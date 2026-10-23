// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::cc_fs;
use crate::cc_fs::ReadIndex;
use crate::device::ChannelStatus;
use crate::diagnostics::support::{self, ChannelDiagnosis, ChannelEvidence};
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::hwmon_repo::{
    AutoCurveInfo, ChannelReadSlots, HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType,
    HwmonDriverInfo,
};
use crate::repositories::hwmon::{apple, auto_curve, devices, probe};
use anyhow::{anyhow, Context, Result};
use futures_util::future::{join3, join_all};
use log::{debug, error, info, log_enabled, trace, warn};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const PATTERN_PWM_FILE_NUMBER: &str = r"^pwm(?P<number>\d+)$";
const PATTERN_FAN_INPUT_FILE_NUMBER: &str = r"^fan(?P<number>\d+)_input$";
pub const PWM_ENABLE_MANUAL_VALUE: u8 = 1;
pub const PWM_ENABLE_AUTO_VALUE: u8 = 2;
pub const PWM_ENABLE_NCT6775_SMART_FAN_IV_VALUE: u8 = 5;

macro_rules! format_fan_input { ($($arg:tt)*) => {{ format!("fan{}_input", $($arg)*) }}; }
macro_rules! format_fan_label { ($($arg:tt)*) => {{ format!("fan{}_label", $($arg)*) }}; }
macro_rules! format_pwm { ($($arg:tt)*) => {{ format!("pwm{}", $($arg)*) }}; }
macro_rules! format_pwm_mode { ($($arg:tt)*) => {{ format!("pwm{}_mode", $($arg)*) }}; }
macro_rules! format_pwm_enable { ($($arg:tt)*) => {{ format!("pwm{}_enable", $($arg)*) }}; }

/// Initialize all applicable fans
pub async fn init_fans(
    base_path: &Path,
    device_name: &str,
    io: &DeviceIo,
) -> Result<Vec<HwmonChannelInfo>> {
    let dir_entries = cc_fs::read_dir(base_path)?;
    let mut fan_caps = HashMap::new();
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        detect_pwm(base_path, file_name, &mut fan_caps, io).await?;
        detect_rpm(base_path, file_name, &mut fan_caps, io).await?;
    }
    let mut fans = caps_to_hwmon_fans(base_path, device_name, fan_caps, io).await?;
    fans.sort_by_key(|c| c.number);
    auto_curve::init_auto_curve_fans(base_path, &mut fans, device_name, io).await?;
    trace!(
        "Hwmon pwm fans detected: {fans:?} for {}",
        base_path.display()
    );
    Ok(fans)
}

/// Diagnoses why one fan channel is or is not controllable.
///
/// `firmware_override_observed` is reserved for the duty-response probe, which
/// is the only thing that can establish it without adding reads to the write
/// path. Passive callers pass `false`.
pub fn diagnose_fan_channel(
    hwmon_name: &str,
    channel: &HwmonChannelInfo,
    firmware_override_observed: bool,
) -> ChannelDiagnosis {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
    if let Some(diagnosis) = apple::diagnose_fan(hwmon_name, channel) {
        // Apple fans are driven through `fanN_output` or `fanN_target`, not `pwmN`, so the
        // pwm-shaped evidence below would condemn a channel we are writing to, and would blame
        // a read-only one on a file its driver never has.
        return diagnosis;
    }
    let evidence = ChannelEvidence {
        has_pwm: channel.caps.has_pwm(),
        pwm_writable: channel.caps.is_fan_controllable(),
        has_rpm: channel.caps.has_rpm(),
        // `pwm_enable_default` is `Some` exactly when the driver exposed a
        // `pwmN_enable` file for this channel.
        has_pwm_enable: channel.pwm_enable_default.is_some(),
    };
    support::diagnose_channel(evidence, hwmon_name, firmware_override_observed)
}

/// Logs the reason a channel cannot be driven, replacing the previous bare
/// "uncontrollable fan found" line. Controllable channels stay silent,
/// consistent with making no noise for working hardware.
///
/// Called from repository init only. `init_fans` is the wrong home for it: the
/// hardware report calls that too, so every report request would re-log the
/// same lines.
pub fn log_uncontrollable_channel(
    base_path: &Path,
    channel: &HwmonChannelInfo,
    diagnosis: &ChannelDiagnosis,
) {
    if diagnosis.verdict.is_controllable() {
        return;
    }
    let Some(evidence) = diagnosis.evidence.as_ref() else {
        // A driver-level verdict has no pwm facts to print.
        info!(
            "Fan channel {} at {} is not controllable: {:?}",
            channel.name,
            base_path.display(),
            diagnosis.verdict,
        );
        return;
    };
    info!(
        "Fan channel {} at {} is not controllable: {:?} (pwm: {}, writable: {}, rpm: {})",
        channel.name,
        base_path.display(),
        diagnosis.verdict,
        evidence.has_pwm,
        evidence.pwm_writable,
        evidence.has_rpm,
    );
}

/// Says why a fan's speed is missing. Repository init only, for the same reason as
/// `log_uncontrollable_channel`: detection stays quiet because the hardware report runs it too.
pub async fn log_unreadable_fan_speeds(base_path: &Path, fans: &[HwmonChannelInfo], io: &DeviceIo) {
    let caps = HwmonChannelCapabilities::empty();
    for channel_number in unread_fan_inputs(base_path, fans) {
        // Detection could not read this one. Reading it again, aloud, gives the reason.
        let _ = get_fan_rpm(io, base_path, &channel_number, &caps, None, None, true).await;
    }
}

/// The fans whose speed file is there and was not readable at detection, in fan order. Detection
/// gives a fan its speed whenever the file reads, so a file without one is a file that did not.
fn unread_fan_inputs(base_path: &Path, fans: &[HwmonChannelInfo]) -> Vec<u8> {
    let Ok(regex_fan_input_file) = Regex::new(PATTERN_FAN_INPUT_FILE_NUMBER) else {
        return Vec::new();
    };
    let Ok(dir_entries) = cc_fs::read_dir(base_path) else {
        return Vec::new();
    };
    let mut channel_numbers: Vec<u8> = dir_entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let os_file_name = entry.file_name();
            let captures = regex_fan_input_file.captures(os_file_name.to_str()?)?;
            captures.name("number")?.as_str().parse().ok()
        })
        .filter(|channel_number| {
            fans.iter()
                .any(|fan| fan.number == *channel_number && fan.caps.has_rpm())
                .not()
        })
        .collect();
    channel_numbers.sort_unstable();
    channel_numbers
}

/// Whether a directory entry names an attribute fan detection reads, `pwmN` or `fanN_input`, so a
/// chip with neither can be ruled out from its listing alone.
pub fn is_fan_attribute(file_name: &str) -> bool {
    // Compiled once. The patterns are constants, so only a typo fails, and the tests catch that.
    static PATTERNS: LazyLock<[Regex; 2]> = LazyLock::new(|| {
        [PATTERN_PWM_FILE_NUMBER, PATTERN_FAN_INPUT_FILE_NUMBER]
            .map(|pattern| Regex::new(pattern).expect("fan attribute patterns are valid"))
    });
    PATTERNS.iter().any(|regex| regex.is_match(file_name))
}

/// Detects if a fan has pwm capability and pwm-write capabilities.
async fn detect_pwm(
    base_path: &Path,
    file_name: &str,
    fan_caps: &mut HashMap<u8, HwmonChannelCapabilities>,
    io: &DeviceIo,
) -> Result<()> {
    let regex_pwm_file = Regex::new(PATTERN_PWM_FILE_NUMBER)?;
    if regex_pwm_file.is_match(file_name).not() {
        return Ok(()); // skip if not a pwm file
    }
    let channel_number: u8 = regex_pwm_file
        .captures(file_name)
        .context("PWM Number should exist")?
        .name("number")
        .context("Number Group should exist")?
        .as_str()
        .parse()?;
    let pwm_path = base_path.join(format_pwm!(channel_number));
    if probe::read_until_ok(&pwm_path, async || try_read_pwm_duty(io, &pwm_path).await)
        .await
        .is_err()
        // Retries exhausted, or the failure was never transient. `get_pwm_duty` has the final
        // say: it owns the auto-mode refusal fallback and the warning.
        && get_pwm_duty(io, base_path, &channel_number, Some(&pwm_path), None, true)
            .await
            .is_none()
    {
        return Ok(()); // skip if pwm file isn't readable
    }
    let pwm_writable = determine_pwm_writable(base_path, channel_number);
    let caps = fan_caps
        .entry(channel_number)
        .or_insert(HwmonChannelCapabilities::empty());
    caps.insert(HwmonChannelCapabilities::PWM);
    caps.set(HwmonChannelCapabilities::FAN_WRITABLE, pwm_writable);
    Ok(())
}

/// Detects if a fan has rpm display capability.
pub async fn detect_rpm(
    base_path: &Path,
    file_name: &str,
    fan_caps: &mut HashMap<u8, HwmonChannelCapabilities>,
    io: &DeviceIo,
) -> Result<()> {
    let regex_fan_input_file = Regex::new(PATTERN_FAN_INPUT_FILE_NUMBER)?;
    if regex_fan_input_file.is_match(file_name).not() {
        return Ok(()); // skip if not a pwm file
    }
    let channel_number: u8 = regex_fan_input_file
        .captures(file_name)
        .context("Fan Number should exist")?
        .name("number")
        .context("Number Group should exist")?
        .as_str()
        .parse()?;
    let rpm_path = base_path.join(format_fan_input!(channel_number));
    // Only readability matters here, so the guard's outcome does not.
    let caps = HwmonChannelCapabilities::empty();
    if probe::read_until_ok(&rpm_path, async || {
        try_read_fan_rpm(io, None, &rpm_path, &caps).await
    })
    .await
    .is_err()
        // Retries exhausted, or the failure was never transient. `get_fan_rpm` has the final say.
        // Quiet: the hardware report detects too. `log_unreadable_fan_speeds` warns at start.
        && get_fan_rpm(io, base_path, &channel_number, &caps, Some(&rpm_path), None, false)
            .await
            .is_none()
    {
        return Ok(()); // skip if rpm file isn't readable
    }
    fan_caps
        .entry(channel_number)
        .or_insert(HwmonChannelCapabilities::empty())
        .insert(HwmonChannelCapabilities::RPM);
    Ok(())
}

/// Converts fan capabilities to `HwmonChannelInfo`
async fn caps_to_hwmon_fans(
    base_path: &Path,
    device_name: &str,
    fan_caps: HashMap<u8, HwmonChannelCapabilities>,
    io: &DeviceIo,
) -> Result<Vec<HwmonChannelInfo>> {
    let mut fans = vec![];
    for (channel_number, mut fan_cap) in fan_caps {
        let pwm_enable_at_init = current_pwm_enable(io, base_path, channel_number, None).await;
        let pwm_enable_default = adjusted_pwm_default(pwm_enable_at_init, device_name);
        let channel_name = get_fan_channel_name(channel_number);
        let label = get_fan_channel_label(base_path, &channel_number, io).await;
        // The raw driver label, so neither sensors.conf nor a user label can lift the rpm guard.
        if label.as_deref().is_some_and(label_names_non_rpm_unit) {
            fan_cap.insert(HwmonChannelCapabilities::NON_RPM_UNIT);
        }
        // deprecated setting:
        // determine_pwm_mode_support(base_path, &channel_number).await;
        // Uncontrollable channels are reported by `log_channel_verdicts` once
        // the full channel is built, which can name a cause instead of a
        // symptom.
        let pwm_path = fan_cap
            .has_pwm()
            .then(|| base_path.join(format_pwm!(channel_number)));
        let rpm_path = fan_cap
            .has_rpm()
            .then(|| base_path.join(format_fan_input!(channel_number)));
        fans.push(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: channel_number,
            pwm_enable_default,
            name: channel_name,
            label,
            caps: fan_cap,
            auto_curve: AutoCurveInfo::None,
            pwm_path,
            rpm_path,
            temp_path: None,
            read_slot: ChannelReadSlots::default(),
        });
    }
    Ok(fans)
}

/// What one fan channel needs read this tick.
///
/// The caller decides, because it depends on the duty cache, which lives with the repository.
/// Reading every channel's pwm every tick is exactly what that cache exists to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanRead {
    /// Real pwm duty and rpm.
    Full,
    /// Rpm only; the caller supplies the duty from its cache.
    RpmOnly,
}

/// One channel's outcome from a batched fan read.
#[derive(Debug)]
pub enum FanReading {
    /// A full read that produced everything expected of the channel.
    Full(ChannelStatus),
    /// An rpm-only read. `None` means the channel has no rpm capability, which is not a failure.
    Rpm(Option<u32>),
    /// Something the channel was expected to report did not read, so the caller should treat this
    /// tick as a miss and let staleness accumulate.
    Failed,
}

/// Reads a set of fan channels in one hop, honouring each channel's own decision.
///
/// Only the attributes the caller asked for are read, so a device whose duty cache is fresh still
/// pays rpm reads alone. The saving is round trips, not attributes: the pass reads exactly what it
/// would have read one channel at a time.
///
/// `want` is asked twice per channel, once to gather the batch and once to walk the reply, so it
/// must answer the same both times. `on_reading` is called once per channel, in order.
pub async fn read_fan_statuses<'a>(
    driver: &'a HwmonDriverInfo,
    channels: impl Iterator<Item = &'a HwmonChannelInfo> + Clone,
    want: impl Fn(&HwmonChannelInfo) -> FanRead,
    mut on_reading: impl FnMut(&'a HwmonChannelInfo, FanReading),
) {
    let slots = channels
        .clone()
        .flat_map(|channel| fan_slots(channel, want(channel)).into_iter().flatten());
    let mut batch = driver.io.read_many(slots).await;
    let mut taken: usize = 0;

    let log_error = log_enabled!(log::Level::Debug);
    for channel in channels {
        let channel_want = want(channel);
        // A channel's slots sit together in the batch, pwm first, as `fan_slots` laid them out.
        let [pwm_slot, rpm_slot] = fan_slots(channel, channel_want);
        let mut take_next = || {
            taken += 1;
            batch.take(taken - 1)
        };
        let pwm_raw = pwm_slot.map(|_| take_next());
        let rpm_raw = rpm_slot.map(|_| take_next());
        let fan_rpm = rpm_raw.and_then(|raw| {
            let raw = raw
                .and_then(check_parsing_32)
                .map(|rpm| guard_fan_rpm(rpm, &channel.caps));
            interpret_fan_rpm(|| rpm_path_for(driver, channel), raw, log_error)
        });
        if channel_want == FanRead::RpmOnly {
            debug_assert!(pwm_raw.is_none());
            // An expected rpm that did not read is a miss; a channel with no rpm at all is not.
            let reading = if channel.caps.has_rpm() && fan_rpm.is_none() {
                FanReading::Failed
            } else {
                FanReading::Rpm(fan_rpm)
            };
            on_reading(channel, reading);
            continue;
        }
        let fan_duty = match pwm_raw {
            Some(raw) => {
                let raw = raw.and_then(check_parsing_8).map(pwm_value_to_duty);
                interpret_pwm_duty(
                    &driver.io,
                    &driver.path,
                    &channel.number,
                    || pwm_path_for(driver, channel),
                    channel.read_slot.pwm_enable,
                    raw,
                    log_error,
                )
                .await
            }
            None => None,
        };
        let expected_pwm_failed = channel.caps.has_pwm() && fan_duty.is_none();
        let expected_rpm_failed = channel.caps.has_rpm() && fan_rpm.is_none();
        let reading = if expected_pwm_failed || expected_rpm_failed {
            FanReading::Failed
        } else {
            FanReading::Full(ChannelStatus {
                name: channel.name.clone(),
                rpm: fan_rpm,
                duty: fan_duty,
                ..Default::default()
            })
        };
        on_reading(channel, reading);
    }
    debug_assert_eq!(
        taken,
        batch.len(),
        "every batched result belongs to a channel"
    );
}

/// The (pwm, rpm) slots one channel adds to a batch, in batch order. The pwm is left out when the
/// caller wants rpm only.
fn fan_slots(channel: &HwmonChannelInfo, want: FanRead) -> [Option<ReadIndex>; 2] {
    let pwm = (want == FanRead::Full)
        .then_some(channel.read_slot.pwm)
        .flatten();
    debug_assert!(want == FanRead::Full || pwm.is_none());
    [pwm, channel.read_slot.rpm]
}

/// Where one channel's pwm value lives.
fn pwm_path_for(driver: &HwmonDriverInfo, channel: &HwmonChannelInfo) -> PathBuf {
    channel
        .pwm_path
        .clone()
        .unwrap_or_else(|| driver.path.join(format_pwm!(channel.number)))
}

/// Where one channel's fan-input value lives.
fn rpm_path_for(driver: &HwmonDriverInfo, channel: &HwmonChannelInfo) -> PathBuf {
    channel
        .rpm_path
        .clone()
        .unwrap_or_else(|| driver.path.join(format_fan_input!(channel.number)))
}

/// Reads pwm-duty and fan-rpm for one channel and returns the
/// resulting `ChannelStatus`, or `None` if all expected fields
/// failed. Pulled out so callers that need per-channel permit
/// handoff (e.g. the preload loop on a slow device) can acquire
/// the device permit just for the duration of one read instead
/// of holding it across the whole device's channel set.
pub async fn read_one_fan_status(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
) -> Option<ChannelStatus> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
    let fan_duty = if channel.caps.has_pwm() {
        get_pwm_duty(
            &driver.io,
            &driver.path,
            &channel.number,
            channel.pwm_path.as_deref(),
            channel.read_slot.pwm_enable,
            log_enabled!(log::Level::Debug),
        )
        .await
    } else {
        None
    };
    let fan_rpm = if channel.caps.has_rpm() {
        get_fan_rpm(
            &driver.io,
            &driver.path,
            &channel.number,
            &channel.caps,
            channel.rpm_path.as_deref(),
            channel.read_slot.rpm,
            log_enabled!(log::Level::Debug),
        )
        .await
    } else {
        None
    };
    let expected_pwm_failed = channel.caps.has_pwm() && fan_duty.is_none();
    let expected_rpm_failed = channel.caps.has_rpm() && fan_rpm.is_none();
    if expected_pwm_failed || expected_rpm_failed {
        return None;
    }
    Some(ChannelStatus {
        name: channel.name.clone(),
        rpm: fan_rpm,
        duty: fan_duty,
        ..Default::default()
    })
}

/// Reads only the RPM portion of a fan channel. Used by the
/// slow-device preload path: when the duty cache is fresh, we can
/// skip the slow PWM duty read and just refresh RPM (which is
/// device-controlled feedback and cannot be cached). Returns
/// `None` if RPM was expected but the read failed; absent RPM cap
/// returns `Some(None)` so the caller can synthesize a status
/// from the cached duty alone.
pub async fn read_one_fan_rpm_only(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
) -> Option<Option<u32>> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
    if channel.caps.has_rpm().not() {
        return Some(None);
    }
    let fan_rpm = get_fan_rpm(
        &driver.io,
        &driver.path,
        &channel.number,
        &channel.caps,
        channel.rpm_path.as_deref(),
        channel.read_slot.rpm,
        log_enabled!(log::Level::Debug),
    )
    .await?;
    Some(Some(fan_rpm))
}

/// Every fan channel in one hop, for callers that want an owned `Vec` (the reinit path).
pub async fn extract_fan_statuses(driver: &HwmonDriverInfo) -> (Vec<ChannelStatus>, bool) {
    let channels = driver
        .channels
        .iter()
        .filter(|channel| channel.hwmon_type == HwmonChannelType::Fan);
    let mut fans = Vec::with_capacity(channels.clone().count());
    let mut any_failure = false;
    let full = |_: &HwmonChannelInfo| FanRead::Full;
    read_fan_statuses(driver, channels, full, |_, reading| {
        if let FanReading::Full(status) = reading {
            fans.push(status);
        } else {
            debug_assert!(matches!(reading, FanReading::Failed));
            any_failure = true;
        }
    })
    .await;
    (fans, any_failure)
}

#[allow(dead_code)]
/// This is the concurrent version of the `extract_fan_statuses` function.
pub async fn extract_fan_statuses_concurrently(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut fan_tasks = vec![];
    moro_local::async_scope!(|scope| {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            let fan_task = scope.spawn(async {
                moro_local::async_scope!(|channel_scope| {
                    let fan_rpm_task = channel_scope.spawn(async {
                        if channel.caps.has_rpm() {
                            get_fan_rpm(
                                &driver.io,
                                &driver.path,
                                &channel.number,
                                &channel.caps,
                                channel.rpm_path.as_deref(),
                                channel.read_slot.rpm,
                                false,
                            )
                            .await
                        } else {
                            None
                        }
                    });
                    let fan_duty_task = channel_scope.spawn(async {
                        if channel.caps.has_pwm() {
                            get_pwm_duty(
                                &driver.io,
                                &driver.path,
                                &channel.number,
                                channel.pwm_path.as_deref(),
                                channel.read_slot.pwm_enable,
                                false,
                            )
                            .await
                        } else {
                            None
                        }
                    });
                    let fan_pwm_mode_task = channel_scope.spawn(async {
                        if channel.caps.has_pwm_mode() {
                            driver
                                .io
                                .read_value(&driver.path.join(format_pwm_mode!(channel.number)))
                                .await
                                .and_then(check_parsing_8)
                                .ok()
                        } else {
                            None
                        }
                    });
                    let (fan_rpm, fan_duty, fan_pwm_mode) =
                        join3(fan_rpm_task, fan_duty_task, fan_pwm_mode_task).await;
                    ChannelStatus {
                        name: channel.name.clone(),
                        rpm: fan_rpm,
                        duty: fan_duty,
                        pwm_mode: fan_pwm_mode,
                        ..Default::default()
                    }
                })
                .await
            });
            fan_tasks.push(fan_task);
        }
        join_all(fan_tasks).await
    })
    .await
}

/// One pwm read with the error intact.
///
/// `get_pwm_duty` adds the auto-mode refusal fallback and the logging on top; detection needs the
/// errno itself, to tell a transient failure from an attribute that is simply not readable.
async fn try_read_pwm_duty(io: &DeviceIo, pwm_path: &Path) -> Result<f64> {
    io.read_value(pwm_path)
        .await
        .and_then(check_parsing_8)
        .map(pwm_value_to_duty)
}

/// One rpm read with the error intact. See `try_read_pwm_duty`.
async fn try_read_fan_rpm(
    io: &DeviceIo,
    slot: Option<ReadIndex>,
    fan_input_path: &Path,
    caps: &HwmonChannelCapabilities,
) -> Result<u32> {
    io.read_one(slot, || fan_input_path.to_path_buf())
        .await
        .and_then(check_parsing_32)
        .map(|rpm| guard_fan_rpm(rpm, caps))
}

/// Whether a failed pwmX read looks like a driver refusing the read in auto mode rather than a
/// read that did not happen. `gpd_fan` returns EOPNOTSUPP and `dell_smm` ENODATA.
///
/// Subtractive, not an allowlist: there is no standard here, so an unfamiliar errno keeps the
/// fallback. Only errnos that provably mean "the read did not happen" are ruled out, or an
/// `EINTR` would be answered with a fabricated 100% duty.
fn is_kernel_refusal(err: &anyhow::Error) -> bool {
    cc_fs::is_transient(err).not()
        && err.downcast_ref::<Error>().is_some_and(|io_err| {
            io_err.raw_os_error().is_some() && io_err.kind() != ErrorKind::NotFound
        })
}

async fn get_pwm_duty(
    io: &DeviceIo,
    base_path: &Path,
    channel_number: &u8,
    pwm_path: Option<&Path>,
    enable_slot: Option<ReadIndex>,
    log_error: bool,
) -> Option<f64> {
    let pwm_path = match pwm_path {
        Some(path) => path,
        None => &base_path.join(format_pwm!(channel_number)),
    };
    let result = try_read_pwm_duty(io, pwm_path).await;
    interpret_pwm_duty(
        io,
        base_path,
        channel_number,
        || pwm_path.to_path_buf(),
        enable_slot,
        result,
        log_error,
    )
    .await
}

/// Turns one raw pwm read into a duty, including the auto-mode carve-out. Shared by the batched
/// pass and the single-channel path. `pwm_path` is built only for a log line.
///
/// The refusal fallback costs a second read, but only on a channel that already failed, so it
/// stays off the batched happy path.
async fn interpret_pwm_duty(
    io: &DeviceIo,
    base_path: &Path,
    channel_number: &u8,
    pwm_path: impl Fn() -> PathBuf,
    enable_slot: Option<ReadIndex>,
    result: Result<f64>,
    log_error: bool,
) -> Option<f64> {
    match result {
        Ok(duty) => {
            debug!("hwmon read {}: {duty}% duty", pwm_path().display());
            Some(duty)
        }
        Err(err) => {
            if is_kernel_refusal(&err) {
                if let Some(pwm_enable) =
                    current_pwm_enable(io, base_path, *channel_number, enable_slot).await
                {
                    if pwm_enable >= PWM_ENABLE_AUTO_VALUE {
                        debug!(
                            "pwmX read refused by kernel driver in auto mode \
                             (pwm_enable={pwm_enable}) at {}; returning 100% duty",
                            pwm_path().display()
                        );
                        return Some(100.0);
                    }
                }
            }
            if log_error {
                warn!(
                    "Could not read fan pwm value at {} ; {err}",
                    pwm_path().display()
                );
            }
            None
        }
    }
}

pub async fn get_fan_rpm(
    io: &DeviceIo,
    base_path: &Path,
    channel_number: &u8,
    caps: &HwmonChannelCapabilities,
    rpm_path: Option<&Path>,
    slot: Option<ReadIndex>,
    log_error: bool,
) -> Option<u32> {
    let fan_input_path = match rpm_path {
        Some(path) => path,
        None => &base_path.join(format_fan_input!(channel_number)),
    };
    let result = try_read_fan_rpm(io, slot, fan_input_path, caps).await;
    interpret_fan_rpm(|| fan_input_path.to_path_buf(), result, log_error)
}

/// Turns one raw fan-input read into an rpm. Shared by the batched pass and the single-channel
/// path so both log and discard failures identically. `fan_input_path` is built only for a log
/// line.
fn interpret_fan_rpm(
    fan_input_path: impl Fn() -> PathBuf,
    result: Result<u32>,
    log_error: bool,
) -> Option<u32> {
    result
        .inspect(|rpm| debug!("hwmon read {}: {rpm} RPM", fan_input_path().display()))
        .inspect_err(|err| {
            if log_error {
                warn!(
                    "Could not read fan rpm value at {}: {err}",
                    fan_input_path().display()
                );
            }
        })
        .ok()
}

/// Not all drivers have `pwm_enable` for their fans. In that case there is no "automatic" mode available.
///  Example `pwm_enable` setting options: (1 and 2 are the most common)
///  - 0 : full speed / off (not used/recommended)
///  - 1 : manual control (setting pwm* will adjust fan speed)
///  - 2 : automatic (primarily used by on-board/chip fan control, like laptops or mobos without smart fan control)
///  - 3 : "Fan Speed Cruise" mode (?)
///  - 4 : "Smart Fan III" mode (NCT6775F only)
///  - 5 : "Smart Fan IV" mode (modern `MoBo`'s with build-in smart fan control probably use this)
/// Reads `pwmN_enable`. `None` when the driver exposes no such file, which
/// means there is no auto mode to hand control back to.
async fn current_pwm_enable(
    io: &DeviceIo,
    base_path: &Path,
    channel_number: u8,
    slot: Option<ReadIndex>,
) -> Option<u8> {
    let pwm_enable_path = base_path.join(format_pwm_enable!(channel_number));
    let current_pwm_enable = io
        .read_one(slot, || pwm_enable_path.clone())
        .await
        .and_then(check_parsing_8)
        .ok();
    if current_pwm_enable.is_none() {
        debug!(
            "No pwm_enable found for fan#{channel_number} at location:{}",
            pwm_enable_path.display()
        );
    }
    current_pwm_enable
}

#[allow(clippy::needless_pass_by_value)]
pub fn check_parsing_8(value: cc_fs::SysfsValue) -> Result<u8> {
    value.parse()
}

#[allow(clippy::needless_pass_by_value)]
pub fn check_parsing_32(value: cc_fs::SysfsValue) -> Result<u32> {
    value.parse()
}

/// A fan input at or above `u16::MAX` is not a speed. 16-bit drivers report `u16::MAX` while a
/// fan spins up, and period-counting drivers overflow far past it on a near-zero count. An input
/// whose driver label names another unit, such as a pressure, passes through.
fn guard_fan_rpm(rpm: u32, caps: &HwmonChannelCapabilities) -> u32 {
    if caps.has_non_rpm_unit() {
        return rpm;
    }
    if rpm < u32::from(u16::MAX) {
        rpm
    } else {
        0
    }
}

/// `nS/cm` and `dL/h` fit with room to spare; longer bracket text is a description, not a unit.
const LABEL_UNIT_CHARS_MAX: usize = 10;

/// The unit a label names in its trailing brackets, e.g. `ubar` in `Pressure [ubar]`.
pub fn label_unit(label: &str) -> Option<&str> {
    let inner = label.trim_end().strip_suffix(']')?;
    let unit = &inner[inner.rfind('[')? + 1..];
    let unit_char_count = unit.chars().count();
    let length_ok = (1..=LABEL_UNIT_CHARS_MAX).contains(&unit_char_count);
    let chars_ok = unit.chars().all(|c| c.is_whitespace().not() && c != ']');
    debug_assert!(unit.contains('[').not());
    (length_ok && chars_ok).then_some(unit)
}

/// Whether a driver label says its fan input is not a speed, as aquacomputer's flow and pressure
/// channels do.
fn label_names_non_rpm_unit(label: &str) -> bool {
    label_unit(label).is_some_and(|unit| unit.eq_ignore_ascii_case("rpm").not())
}

/// If a `HWMon` driver has not set the writable bit on the sysfs file, then that
/// indicates that the pwm value is read-only and not configurable.
fn determine_pwm_writable(base_path: &Path, channel_number: u8) -> bool {
    let pwm_path = base_path.join(format_pwm!(channel_number));
    let pwm_writable = cc_fs::metadata(&pwm_path)
        .inspect_err(|_| error!("PWM file metadata is not readable: {}", pwm_path.display()))
        // This check should be sufficient, as we're running as root:
        .is_ok_and(|att| att.permissions().readonly().not());
    if pwm_writable.not() {
        info!(
            "PWM fan at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
            pwm_path.display()
        );
    }
    pwm_writable
}

/// We save the existing `pwm_enable` setting and applying the Default Profile/shutting down the
/// service will then revert to that setting - which is usually 'auto' set by the bios on boot -
/// but not necessarily and not all devices support an auto setting.
///
/// This means we can not safely apply 'auto' to `pwm_enable` indiscriminately and therefor we use
/// whatever the initial setting was as the Default.
///
/// Note: Some drivers should have an automatic fallback for safety reasons,
/// regardless of the current value.
fn adjusted_pwm_default(current_pwm_enable: Option<u8>, device_name: &str) -> Option<u8> {
    current_pwm_enable.map(|original_value| {
        if devices::device_needs_pwm_fallback(device_name) {
            2
        } else {
            original_value
        }
    })
}

/// Reads the contents of the fan?_label file specified by `base_path` and
/// `channel_number`, trims any leading or trailing whitespace, and returns the resulting string if it
/// is not empty.
///
/// Arguments:
///
/// * `base_path`: A `PathBuf` object representing the base path where the file `fan{}_label` is
///   located.
/// * `channel_number`: The `channel_number` parameter is an unsigned 8-bit integer that represents
///   the channel number. It is used to construct the file path for reading the label.
///
/// Returns:
///
/// an `Option<String>`.
pub async fn get_fan_channel_label(
    base_path: &Path,
    channel_number: &u8,
    io: &DeviceIo,
) -> Option<String> {
    // On the device's thread: the label comes from the driver, which may have to ask the hardware.
    io.read_text(&base_path.join(format_fan_label!(channel_number)))
        .await
        .ok()
        .and_then(|label| {
            let fan_label = label.trim();
            if fan_label.is_empty() {
                warn!(
                    "Fan label is empty for {}/fan{channel_number}_label",
                    base_path.display()
                );
                None
            } else {
                Some(fan_label.to_string())
            }
        })
}

/// Returns a string that represents a unique channel name/ID.
///
/// Arguments:
///
/// * `channel_number`: The `channel_number` parameter is a reference to an unsigned 8-bit integer
///   (`&u8`).
///
/// Returns:
///
/// * A `String` that represents a unique channel name/ID.
pub fn get_fan_channel_name(channel_number: u8) -> String {
    format!("fan{channel_number}")
}

/// This sets `pwm_enable` to the default value,
/// unless it's currently set to "auto-mode" (>1), then it will be left on auto mode.
/// This is mostly used when shutting down the service to revert to the default value,
/// but not necessarily and not all devices support an auto setting.
pub async fn set_pwm_enable_to_default_or_auto(
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    io: &DeviceIo,
) -> Result<()> {
    let Some(default_value) = channel_info.pwm_enable_default else {
        // not all devices have pwm_enable available
        return Ok(());
    };
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    let current_pwm_enable = io
        .read_value(&path_pwm_enable)
        .await
        .and_then(check_parsing_8)?;
    if current_pwm_enable < PWM_ENABLE_AUTO_VALUE && current_pwm_enable != default_value {
        if let Err(err) = write_pwm_enable(&path_pwm_enable, default_value, io).await {
            warn!("Failed to reset pwm_enable to default: {err}");
        }
    }
    debug!(
        "Reset Hwmon value at {}/pwm{}_enable to default/auto value",
        base_path.display(),
        channel_info.number
    );
    Ok(())
}

/// This sets `pwm_enable` to the desired value. Unlike other operations,
/// it will not check if it's already set to the desired value.
/// See also `current_pwm_enable`.
pub async fn set_pwm_enable(
    pwm_enable_value: u8,
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    io: &DeviceIo,
) -> Result<()> {
    if channel_info.pwm_enable_default.is_none() {
        // not all devices have pwm_enable available
        return Ok(());
    }
    if pwm_enable_value > 5 {
        return Err(anyhow!(
            "pwm_enable value must be between 0 and 5 (inclusive)"
        ));
    }
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    write_pwm_enable(&path_pwm_enable, pwm_enable_value, io).await
}

/// This sets `pwm_enable` to the desired value if it's not already set to the desired value.
/// See also `current_pwm_enable`.
pub async fn set_pwm_enable_if_not_already(
    pwm_enable_value: u8,
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    io: &DeviceIo,
) -> Result<()> {
    if channel_info.pwm_enable_default.is_none() {
        // not all devices have pwm_enable available
        return Ok(());
    }
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    let current_pwm_enable = io
        .read_value(&path_pwm_enable)
        .await
        .and_then(check_parsing_8)?;
    if current_pwm_enable == pwm_enable_value {
        Ok(())
    } else {
        write_pwm_enable(&path_pwm_enable, pwm_enable_value, io).await
    }
}

async fn write_pwm_enable(
    path_pwm_enable: &Path,
    pwm_enable_value: u8,
    io: &DeviceIo,
) -> Result<()> {
    io.write_value(path_pwm_enable, pwm_enable_value.to_string().into_bytes())
        .await
        .inspect(|()| {
            debug!(
                "Applied pwm_enable for {} of {pwm_enable_value}",
                path_pwm_enable.display()
            );
        })
        .map_err(|err| {
            anyhow!(
                "Unable to set pwm_enable for {} to {pwm_enable_value}. \
                    Most likely because of a limitation set by the driver or a BIOS setting; \
                    Error: {err}",
                path_pwm_enable.display()
            )
        })
}

pub async fn set_pwm_duty(
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    speed_duty: u8,
    io: &DeviceIo,
) -> Result<()> {
    let pwm_value = duty_to_pwm_value(speed_duty);
    let pwm_path: &Path = match channel_info.pwm_path.as_deref() {
        Some(path) => path,
        None => &base_path.join(format_pwm!(channel_info.number)),
    };
    io.write_value(pwm_path, pwm_value.to_string().into_bytes())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set PWM value {pwm_value} for {} Reason: {err}",
                pwm_path.display()
            )
        })
}

/// Converts a pwm value (0-255) to a duty value (0-100%)
pub fn pwm_value_to_duty(pwm_value: u8) -> f64 {
    ((f64::from(pwm_value) / 0.255).round() / 10.0).round()
}

/// Converts a duty value (0-100%) to a pwm value (0-255)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn duty_to_pwm_value(speed_duty: u8) -> u8 {
    let clamped_duty = f64::from(speed_duty.clamp(0, 100));
    // round only takes the first decimal digit into consideration, so we adjust to have it take the first two digits into consideration.
    ((clamped_duty * 25.5).round() / 10.0).round() as u8
}

/// Tests
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::diagnostics::support::ChannelVerdict;
    use crate::repositories::hwmon::drivetemp;
    use serial_test::serial;
    use std::path::{Path, PathBuf};

    struct HwmonFileContext {
        /// Owns the fixture directory, which is removed when the context drops.
        _dir: tempfile::TempDir,
        test_base_path: PathBuf,
    }

    fn setup() -> HwmonFileContext {
        let dir = tempfile::tempdir().unwrap();
        let test_base_path = dir.path().to_path_buf();
        HwmonFileContext {
            _dir: dir,
            test_base_path,
        }
    }

    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn find_fan_dir_not_exist() {
        cc_fs::test_runtime(async {
            // given:
            let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(&test_base_path, &device_name, &DeviceIo::default()).await;

            // then:
            assert!(fans_result.is_err());
            assert!(fans_result
                .map_err(|err| err.to_string().contains("No such file or directory"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn find_fan() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            cc_fs::write(
                test_base_path.join("fan1_input"),
                b"3000".to_vec(), // rpm
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name, &DeviceIo::default()).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].caps.has_pwm_mode().not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].caps.is_fan_controllable());
        });
    }

    #[test]
    #[serial]
    fn find_fan_pwm_only() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name, &DeviceIo::default()).await;

            // then:
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].caps.has_pwm_mode().not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].caps.is_fan_controllable());
        });
    }

    /// Detection is quiet, so the start-up warning has to find by itself the speeds detection
    /// could not read. Detects a device with a readable speed, unreadable ones with and
    /// without a pwm, and files that only look like a speed, then asks which are unread.
    #[test]
    #[serial]
    fn only_a_speed_detection_could_not_read_is_unread() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let dir = &ctx.test_base_path;
            for (name, value) in [
                ("fan1_input", "1200"),
                ("fan2_input", "not a number"),
                ("pwm3", "128"),
                ("fan3_input", "not a number"),
                ("fan10_input", ""),
                ("fan4_label", "no speed file"),
                ("temp1_input", "not a number"),
            ] {
                cc_fs::write(dir.join(name), value.as_bytes().to_vec())
                    .await
                    .unwrap();
            }

            let fans = init_fans(dir, "Test Driver", &DeviceIo::default())
                .await
                .unwrap();
            let unread = unread_fan_inputs(dir, &fans);
            let unread_with_no_fans = unread_fan_inputs(dir, &[]);

            let detected: Vec<(u8, bool)> = fans
                .iter()
                .map(|fan| (fan.number, fan.caps.has_rpm()))
                .collect();
            assert_eq!(detected, [(1, true), (3, false)]);
            assert_eq!(unread, [2, 3, 10]);
            assert_eq!(unread_with_no_fans, [1, 2, 3, 10]);
        });
    }

    /// A device that is gone has nothing to warn about, and must not fail the start-up that
    /// asks. Asks about a directory that does not exist.
    #[test]
    #[serial]
    fn a_missing_device_has_no_unread_speeds() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let missing = ctx.test_base_path.join("missing");

            let unread = unread_fan_inputs(&missing, &[]);
            log_unreadable_fan_speeds(&missing, &[], &DeviceIo::default()).await;

            assert!(unread.is_empty());
        });
    }

    #[test]
    #[serial]
    fn find_fan_rpm_only() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("fan1_input"),
                b"3000".to_vec(), // rpm
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name, &DeviceIo::default()).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].caps.has_pwm_mode().not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].caps.is_fan_controllable().not());
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_to_default() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"1".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable_to_default_or_auto(
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "2");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_to_default_doesnt_exist() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable_to_default_or_auto(
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let pwm_enable_doesnt_exist = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .is_err();
            assert!(result.is_ok());
            assert!(pwm_enable_doesnt_exist);
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_to_default_auto() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            // current value is already auto (2)
            cc_fs::write(test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();
            // default is manual (1) but should not override auto
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(1),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable_to_default_or_auto(
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            assert!(result.is_ok());
            // remains on auto (2)
            assert_eq!(current_pwm_enable, "2");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable(
                PWM_ENABLE_MANUAL_VALUE,
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_doesnt_exist() {
        // test to make sure we don't return an Err if pwm_enable doesn't exist
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable(
                PWM_ENABLE_MANUAL_VALUE,
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let pwm_enable_doesnt_exist = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .is_err();
            assert!(result.is_ok());
            assert!(pwm_enable_doesnt_exist);
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_if_not_already_set() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result = set_pwm_enable_if_not_already(
                PWM_ENABLE_MANUAL_VALUE,
                test_base_path,
                &channel_info,
                &DeviceIo::default(),
            )
            .await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result =
                set_pwm_duty(test_base_path, &channel_info, 50, &DeviceIo::default()).await;

            // then:
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(format!("{current_duty:.1}"), "50.0");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_duty_using_path() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result =
                set_pwm_duty(test_base_path, &channel_info, 50, &DeviceIo::default()).await;

            // then:
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(format!("{current_duty:.1}"), "50.0");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_duty_no_pwm_enable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            };

            // when:
            let result =
                set_pwm_duty(test_base_path, &channel_info, 50, &DeviceIo::default()).await;

            // then:
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            assert!(result.is_ok());
            assert_eq!(current_duty.to_string(), "50");
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_returns_duty_when_file_valid() {
        // Verifies the happy path: a readable pwm file with a valid u8 value is converted to
        // a duty percentage and returned as Some.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 = 255 (full speed)
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                true,
            )
            .await;

            // then:
            assert_eq!(result, Some(100.0));
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_returns_none_when_file_missing() {
        // Verifies the negative space: when the pwm sysfs file does not exist (ENOENT),
        // the function returns None without panicking.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: no pwm1 file exists

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then:
            assert_eq!(result, None);
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_returns_none_when_file_has_invalid_content() {
        // Verifies that a pwm file containing non-numeric content (parse error) returns None
        // rather than propagating the error upward.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 contains text that cannot be parsed as u8
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"not_a_number".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then:
            assert_eq!(result, None);
        });
    }

    /// Goal: a transient errno must never be answered with a fabricated 100% duty. Before this,
    /// `is_kernel_refusal` accepted every errno but ENOENT, so an `EINTR` from an interrupted
    /// sysfs read on a driver with `pwmN_enable >= 2` reported a full-speed fan that was never
    /// read. Method: classify one error per errno; a real EINTR cannot be produced from a
    /// regular file, so the predicate is exercised directly.
    #[test]
    fn transient_errnos_are_not_kernel_refusals() {
        for errno in [
            nix::libc::EINTR,
            nix::libc::ETIMEDOUT,
            nix::libc::EAGAIN,
            nix::libc::EBUSY,
        ] {
            let err: anyhow::Error = Error::from_raw_os_error(errno).into();
            assert!(
                is_kernel_refusal(&err).not(),
                "errno {errno} must not fabricate a duty"
            );
        }
    }

    /// Goal: the fallback the gpd_fan and dell_smm channels depend on must survive the change,
    /// including for an errno we have never seen, since drivers agree on no standard here.
    /// ENOENT stays excluded: a missing pwm file means no channel, not a refusal. Method:
    /// classify one error per errno.
    #[test]
    fn refusal_errnos_still_reach_the_auto_mode_fallback() {
        for errno in [
            nix::libc::EOPNOTSUPP,
            nix::libc::ENODATA,
            nix::libc::EACCES,
            nix::libc::EIO,
        ] {
            let err: anyhow::Error = Error::from_raw_os_error(errno).into();
            assert!(is_kernel_refusal(&err), "errno {errno} lost the fallback");
        }
        let missing: anyhow::Error = Error::from_raw_os_error(nix::libc::ENOENT).into();
        assert!(is_kernel_refusal(&missing).not());
        let parse_err: anyhow::Error = anyhow::anyhow!("invalid digit found in string");
        assert!(is_kernel_refusal(&parse_err).not());
    }

    #[test]
    #[serial]
    fn get_pwm_duty_pwm_readable_in_auto_mode() {
        // Verifies the happy path: when pwm is readable in auto mode, the normal duty value
        // is returned without triggering the fallback.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 = 128, pwm1_enable = 2 (auto)
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then: normal read succeeds, no fallback needed
            assert_eq!(result, Some(pwm_value_to_duty(128)));
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_none_when_file_missing_with_auto_mode() {
        // Verifies that ENOENT (file missing) is excluded from the auto-mode fallback.
        // A missing pwm file means the channel doesn't exist, not a driver refusal.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: no pwm1 file, but pwm1_enable = 2 (auto)
            cc_fs::write(ctx.test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then: ENOENT is not a kernel refusal — must return None
            assert_eq!(result, None);
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_none_when_file_missing_no_pwm_enable() {
        // Verifies that ENOENT returns None even when pwm_enable doesn't exist.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: no pwm1 file, no pwm1_enable file

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then:
            assert_eq!(result, None);
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_none_when_parse_error_and_auto_mode() {
        // Verifies that parse errors (InvalidData) are excluded from the auto-mode fallback.
        // A garbled file is not a kernel driver refusal.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 has non-numeric content, pwm1_enable = 2 (auto)
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"bad".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then: parse error has no raw_os_error — must return None
            assert_eq!(result, None);
        });
    }

    #[test]
    #[serial]
    fn get_pwm_duty_none_when_parse_error_and_manual_mode() {
        // Verifies that parse errors return None regardless of pwm_enable value.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 has non-numeric content, pwm1_enable = 1 (manual)
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"bad".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("pwm1_enable"), b"1".to_vec())
                .await
                .unwrap();

            // when:
            let result = get_pwm_duty(
                &DeviceIo::default(),
                &ctx.test_base_path,
                &1,
                None,
                None,
                false,
            )
            .await;

            // then: parse error + manual mode = no fallback
            assert_eq!(result, None);
        });
    }

    // --- extract_fan_statuses: failure indicator ---
    async fn make_driver(base_path: &Path, channels: Vec<HwmonChannelInfo>) -> HwmonDriverInfo {
        HwmonDriverInfo {
            name: "test_driver".to_string(),
            path: base_path.to_path_buf(),
            model: None,
            u_id: String::new(),
            channels,
            drivetemp: drivetemp::DrivetempState::default(),
            apple_fans: None,
            io: DeviceIo::default(),
        }
        .with_read_registry()
        .await
    }

    fn fan_channel(number: u8, caps: HwmonChannelCapabilities) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            pwm_enable_default: None,
            name: format!("fan{number}"),
            label: None,
            caps,
            auto_curve: AutoCurveInfo::None,
            pwm_path: None,
            rpm_path: None,
            temp_path: None,
            read_slot: ChannelReadSlots::default(),
        }
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_no_failure_when_all_reads_succeed() {
        // Verifies that when all fan channels read successfully, the failure
        // indicator is false and values are populated.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 and fan1_input exist with valid values.
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert!(statuses[0].duty.is_some());
            assert!(statuses[0].rpm.is_some());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_failure_when_pwm_missing() {
        // Verifies that a channel with PWM capability but no pwm file
        // triggers a failure and is omitted from the returned statuses
        // so the upstream failsafe overlay can substitute safe values.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1_input exists, but no pwm1.
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure);
            assert!(statuses.is_empty());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_failure_when_rpm_missing() {
        // Verifies that a channel with RPM capability but no fan_input
        // file triggers a failure and is omitted from the returned
        // statuses, for the same reason as the PWM-missing case.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: pwm1 exists, but no fan1_input.
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure);
            assert!(statuses.is_empty());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_failure_when_both_pwm_and_rpm_missing() {
        // Regression: when both expected reads fail, the fan channel
        // must be omitted entirely. Previously an all-`None` entry was
        // pushed, which blocked the failsafe overlay from substituting
        // the configured failsafe values and the channel disappeared
        // from downstream status views.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: neither pwm1 nor fan1_input exist.
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure);
            assert!(statuses.is_empty());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_no_failure_for_rpm_only_channel_without_pwm_cap() {
        // Verifies that a channel without PWM capability does not trigger
        // a failure when pwm returns None (that is expected).
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1_input exists, no pwm1, and channel only has RPM cap.
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"900".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert!(statuses[0].duty.is_none());
            assert!(statuses[0].rpm.is_some());
        });
    }

    // --- extract_fan_statuses: ordering and failures ---
    #[test]
    #[serial]
    fn extract_fan_statuses_preserves_channel_order() {
        // Verifies the streaming variant invokes the sink once per
        // successful channel in channel-number order, matching the
        // buffered version's Vec order.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: three successive fan channels, all readable.
            for number in 1u8..=3 {
                cc_fs::write(
                    ctx.test_base_path.join(format!("pwm{number}")),
                    b"128".to_vec(),
                )
                .await
                .unwrap();
                cc_fs::write(
                    ctx.test_base_path.join(format!("fan{number}_input")),
                    b"1200".to_vec(),
                )
                .await
                .unwrap();
            }
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(
                &ctx.test_base_path,
                vec![
                    fan_channel(1, caps.clone()),
                    fan_channel(2, caps.clone()),
                    fan_channel(3, caps),
                ],
            )
            .await;

            // when: collect invocations in order.
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            assert!(any_failure.not());
            assert_eq!(received, vec!["fan1", "fan2", "fan3"]);
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_skips_failed_channels() {
        // Verifies the sink is not invoked for a channel whose
        // expected read fails, and any_failure reflects it.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1 readable, fan2 missing its pwm file.
            cc_fs::write(ctx.test_base_path.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            cc_fs::write(ctx.test_base_path.join("fan2_input"), b"900".to_vec())
                .await
                .unwrap();
            // no pwm2 file, but fan2 has PWM cap -> expected-field failure
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(
                &ctx.test_base_path,
                vec![fan_channel(1, caps.clone()), fan_channel(2, caps)],
            )
            .await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            assert!(any_failure);
            assert_eq!(received, vec!["fan1"]);
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_empty_when_no_channels() {
        // Verifies the sink is never invoked for a driver with no
        // fan channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup();
            let driver = make_driver(&ctx.test_base_path, vec![]).await;

            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            assert!(statuses.is_empty());
            assert!(any_failure.not());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_no_failure_when_no_fan_channels() {
        // Verifies that an empty channel list produces no statuses
        // and no failure.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: driver with no channels.
            let driver = make_driver(&ctx.test_base_path, vec![]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure.not());
            assert!(statuses.is_empty());
        });
    }

    // --- rpm guard ---

    #[test]
    fn guard_fan_rpm_zeroes_values_at_or_above_u16_max() {
        // Goal: the guard passes real speeds and zeroes the spin-up value and overflow garbage.
        // Method: check both sides of the u16::MAX boundary and a count-based overflow value.
        let caps = HwmonChannelCapabilities::RPM;
        assert_eq!(guard_fan_rpm(0, &caps), 0);
        assert_eq!(guard_fan_rpm(1200, &caps), 1200);
        assert_eq!(guard_fan_rpm(65_534, &caps), 65_534);
        assert_eq!(guard_fan_rpm(65_535, &caps), 0);
        assert_eq!(guard_fan_rpm(1_350_000, &caps), 0);
    }

    #[test]
    fn guard_fan_rpm_passes_labelled_non_rpm_units() {
        // Goal: an input whose driver label names another unit is never zeroed.
        // Method: Leakshield-sized pressures and the u16::MAX value pass through unchanged.
        let caps = HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::NON_RPM_UNIT;
        assert_eq!(guard_fan_rpm(438_300, &caps), 438_300);
        assert_eq!(guard_fan_rpm(3_276_700, &caps), 3_276_700);
        assert_eq!(guard_fan_rpm(65_535, &caps), 65_535);
        assert_eq!(guard_fan_rpm(0, &caps), 0);
    }

    #[test]
    fn label_unit_reads_short_trailing_brackets() {
        // Goal: only a short, space-free trailing bracket is a unit.
        // Method: aquacomputer labels, rpm in any case, plain labels and descriptive brackets.
        assert_eq!(label_unit("Pressure [ubar]"), Some("ubar"));
        assert_eq!(label_unit("Flow speed [dL/h]"), Some("dL/h"));
        assert_eq!(label_unit("Conductivity [nS/cm] "), Some("nS/cm"));
        assert_eq!(label_unit("Fan [RPM]"), Some("RPM"));
        assert_eq!(label_unit("CPU Fan"), None);
        assert_eq!(label_unit("Fan [Rear Exhaust]"), None);
        assert_eq!(label_unit("Fan [toolongunit1]"), None);
        assert_eq!(label_unit("Fan []"), None);
        assert_eq!(label_unit("Fan ]"), None);
        assert_eq!(label_unit("Fan [x]]"), None);

        assert!(label_names_non_rpm_unit("Pressure [ubar]"));
        assert!(label_names_non_rpm_unit("Water quality [%]"));
        assert!(label_names_non_rpm_unit("Fan [RPM]").not());
        assert!(label_names_non_rpm_unit("Fan [rpm]").not());
        assert!(label_names_non_rpm_unit("Pump speed").not());
    }

    #[test]
    #[serial]
    fn find_fan_flags_non_rpm_unit_from_driver_label() {
        // Goal: detection marks an input as non-rpm only when its driver label names a unit.
        // Method: a Leakshield-style pressure channel next to a plainly labelled fan.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1 is a pressure in ubar, fan2 a real fan.
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("fan1_input"), b"438300".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_label"), b"Pressure [ubar]\n".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan2_input"), b"1200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan2_label"), b"Pump speed\n".to_vec())
                .await
                .unwrap();

            // when:
            let fans = init_fans(base, "leakshield", &DeviceIo::default()).await;

            // then:
            let fans = fans.unwrap();
            assert_eq!(fans.len(), 2);
            assert!(fans[0].caps.has_rpm());
            assert!(fans[0].caps.has_non_rpm_unit());
            assert!(fans[1].caps.has_rpm());
            assert!(fans[1].caps.has_non_rpm_unit().not());
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_passes_labelled_non_rpm_values() {
        // Goal: the batched read keeps a labelled pressure intact instead of zeroing it.
        // Method: a NON_RPM_UNIT channel holding a Leakshield-sized pressure.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"438300".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::NON_RPM_UNIT;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].rpm, Some(438_300));
        });
    }

    #[test]
    #[serial]
    fn read_one_fan_status_passes_labelled_non_rpm_values() {
        // Goal: the single-channel read keeps a labelled pressure intact, matching the batch.
        // Method: read a Leakshield-sized pressure through `read_one_fan_status`.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"438300".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::NON_RPM_UNIT;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let status = read_one_fan_status(&driver, &driver.channels[0]).await;

            // then:
            assert_eq!(status.and_then(|status| status.rpm), Some(438_300));
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_zeroes_rpm_at_or_above_u16_max() {
        // Goal: the per-tick batched read applies the same rpm guard as the single-channel read.
        // Method: a fan input holding a count-based overflow value must come back as 0 rpm.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1_input holds 1350000, a value no fan can spin at.
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"1350000".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let (statuses, any_failure) = extract_fan_statuses(&driver).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].rpm, Some(0));
        });
    }

    #[test]
    #[serial]
    fn read_one_fan_status_zeroes_rpm_at_or_above_u16_max() {
        // Goal: the single-channel read keeps the rpm guard, matching the batched read.
        // Method: read the spin-up value u16::MAX through `read_one_fan_status`.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1_input holds the spin-up value.
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"65535".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let status = read_one_fan_status(&driver, &driver.channels[0]).await;

            // then:
            assert_eq!(status.and_then(|status| status.rpm), Some(0));
        });
    }

    #[test]
    #[serial]
    fn read_one_fan_rpm_only_passes_labelled_non_rpm_values() {
        // Goal: the slow-device rpm-only read keeps a labelled pressure intact.
        // Method: read a Leakshield-sized pressure through `read_one_fan_rpm_only`.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"438300".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::NON_RPM_UNIT;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let rpm = read_one_fan_rpm_only(&driver, &driver.channels[0]).await;

            // then:
            assert_eq!(rpm, Some(Some(438_300)));
        });
    }

    #[test]
    #[serial]
    fn read_one_fan_rpm_only_zeroes_rpm_at_or_above_u16_max() {
        // Goal: the slow-device rpm-only read keeps the rpm guard.
        // Method: read the spin-up value u16::MAX through `read_one_fan_rpm_only`.
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given: fan1_input holds the spin-up value.
            cc_fs::write(ctx.test_base_path.join("fan1_input"), b"65535".to_vec())
                .await
                .unwrap();
            let caps = HwmonChannelCapabilities::RPM;
            let driver = make_driver(&ctx.test_base_path, vec![fan_channel(1, caps)]).await;

            // when:
            let rpm = read_one_fan_rpm_only(&driver, &driver.channels[0]).await;

            // then:
            assert_eq!(rpm, Some(Some(0)));
        });
    }

    /// Goal: an Apple SMC fan is driven through `fanN_output` and never has a
    /// `pwmN`, so the pwm-shaped evidence must not condemn it. Method: build
    /// the capability set `AppleSmcLegacy::detect_apple_smc_fans` produces and
    /// check the verdict, which in a debug build also exercises the assertion
    /// that a writable pwm implies a pwm.
    #[test]
    fn apple_smc_fan_without_pwm_is_controllable() {
        let channel = HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            name: "fan1".to_string(),
            caps: HwmonChannelCapabilities::APPLE_SMC
                | HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::RPM,
            ..Default::default()
        };

        let diagnosis = diagnose_fan_channel("macsmc_hwmon", &channel, false);

        assert_eq!(diagnosis.verdict, ChannelVerdict::Controllable);
        // No pwm file means no pwm facts to report, not four false ones.
        assert!(diagnosis.evidence.is_none());
    }

    /// Goal: an Apple fan that cannot be driven is not blamed on a missing `pwmN`, a file its
    /// driver never has. Method: a fan whose control file is read-only and one with no control
    /// at all, under an Apple name.
    #[test]
    fn an_uncontrollable_apple_fan_is_not_judged_by_pwm() {
        let read_only = HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            name: "fan1".to_string(),
            caps: HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::APPLE_SMC_READ_ONLY,
            ..Default::default()
        };
        let speed_only = HwmonChannelInfo {
            caps: HwmonChannelCapabilities::RPM,
            ..read_only.clone()
        };

        let read_only_diagnosis = diagnose_fan_channel("macsmc_hwmon", &read_only, false);
        let speed_only_diagnosis = diagnose_fan_channel("applesmc", &speed_only, false);

        assert_eq!(read_only_diagnosis.verdict, ChannelVerdict::PwmReadOnly);
        assert!(read_only_diagnosis.evidence.is_none());
        assert_eq!(
            speed_only_diagnosis.verdict,
            ChannelVerdict::NotSupportedByDriver
        );
        assert!(speed_only_diagnosis.evidence.is_none());
    }

    /// Goal: the negative space of the case above. A fan that only reports
    /// speed is still `NoPwm`, so the Apple branch cannot swallow a genuinely
    /// uncontrollable channel.
    #[test]
    fn rpm_only_fan_is_still_condemned() {
        let channel = HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: 9,
            name: "fan9".to_string(),
            caps: HwmonChannelCapabilities::RPM,
            ..Default::default()
        };

        let diagnosis = diagnose_fan_channel("aquacomputer_d5next", &channel, false);

        assert_eq!(diagnosis.verdict, ChannelVerdict::NoPwm);
        assert!(diagnosis.evidence.is_some());
    }

    /// Goal: a fan read builds its path only for a log line, since every tick reads every fan and
    /// the path is a heap allocation. Method: with warnings enabled, interpret a failed read that
    /// is not logged, a good read, then a failed read that is logged, through a path builder that
    /// counts its calls.
    #[test]
    #[serial]
    fn a_fan_read_builds_its_path_only_to_log() {
        let level = crate::logger::MaxLevelGuard::set(log::LevelFilter::Warn);
        let built = std::cell::Cell::new(0);
        let path = || {
            built.set(built.get() + 1);
            PathBuf::from("/sys/class/hwmon/hwmon1/fan1_input")
        };

        let failed = interpret_fan_rpm(path, Err(anyhow!("no such device")), false);
        let read = interpret_fan_rpm(path, Ok(1200), false);
        let built_unlogged = built.get();
        let logged = interpret_fan_rpm(path, Err(anyhow!("no such device")), true);
        drop(level);

        assert_eq!(failed, None);
        assert_eq!(read, Some(1200));
        assert_eq!(built_unlogged, 0);
        assert_eq!(logged, None);
        assert_eq!(built.get(), 1);
    }

    /// Goal: an rpm-only channel between two full ones adds one slot to the batch, not two, so
    /// every channel after it must still read its own pwm and rpm. A shifted reply would report a
    /// neighbour's speed under this channel's name. Method: give each attribute a distinct value,
    /// ask for full, rpm-only, full, and check each reading.
    #[test]
    #[serial]
    fn mixed_fan_decisions_keep_each_channel_its_own_values() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            for (number, pwm, rpm) in [(1, "255", "1100"), (2, "128", "2200"), (3, "51", "3300")] {
                let pwm_path = ctx.test_base_path.join(format!("pwm{number}"));
                cc_fs::write(pwm_path, pwm.as_bytes().to_vec())
                    .await
                    .unwrap();
                let rpm_path = ctx.test_base_path.join(format!("fan{number}_input"));
                cc_fs::write(rpm_path, rpm.as_bytes().to_vec())
                    .await
                    .unwrap();
            }
            let caps = HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM;
            let driver = make_driver(
                &ctx.test_base_path,
                vec![
                    fan_channel(1, caps.clone()),
                    fan_channel(2, caps.clone()),
                    fan_channel(3, caps),
                ],
            )
            .await;
            let want = |channel: &HwmonChannelInfo| {
                if channel.number == 2 {
                    FanRead::RpmOnly
                } else {
                    FanRead::Full
                }
            };
            let mut readings = Vec::new();

            read_fan_statuses(&driver, driver.channels.iter(), want, |channel, reading| {
                readings.push((channel.number, reading));
            })
            .await;

            assert_eq!(readings.len(), 3);
            let FanReading::Full(first) = &readings[0].1 else {
                panic!("fan1 must read in full, got {:?}", readings[0].1);
            };
            assert_eq!((first.duty, first.rpm), (Some(100.0), Some(1100)));
            assert!(
                matches!(readings[1].1, FanReading::Rpm(Some(2200))),
                "fan2 must read its rpm only, got {:?}",
                readings[1].1
            );
            let FanReading::Full(third) = &readings[2].1 else {
                panic!("fan3 must read in full, got {:?}", readings[2].1);
            };
            assert_eq!((third.duty, third.rpm), (Some(20.0), Some(3300)));
        });
    }
}
