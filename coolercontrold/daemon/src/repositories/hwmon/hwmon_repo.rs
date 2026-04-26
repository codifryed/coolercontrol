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

//! `HWMon` repository: device discovery, status reads, and fan
//! writes for sysfs-exposed sensors.
//!
//! This module owns three coupled concurrency primitives per
//! device. Together they bound how long any one operation can
//! stall the daemon and ensure a slow device does not pile up
//! work.
//!
//! ## Per-device permit (`device_permits: HashMap<TypeIndex, Rc<Semaphore>>`)
//!
//! One `Rc<Semaphore>` of capacity 1 per device. All sysfs reads
//! and writes for that device must hold it; only one operation
//! is in flight at a time.
//!
//! Crucially, the permit is held *per channel*, not per
//! whole-device-preload: `preload_device_statuses` acquires the
//! permit inside `read_one_channel`, runs that one channel's read,
//! and releases. This lets the writer task slip in between
//! channel reads, so on a slow device worst-case write latency
//! is one channel-read time rather than `N_channels * read_time`.
//! Reads are idempotent, so the snapshot semantics survive the
//! handoff (channels were already read sequentially; the change
//! is just a release point between each).
//!
//! ## Two timeouts on the permit
//!
//! Read permit: `poll_rate * READ_PERMIT_RATIO` (0.7), so 700 ms
//! at the default 1 s tick. Applied per channel inside the
//! preload loop: a channel that cannot acquire within this
//! budget skips this tick. The cache + per-channel staleness
//! counter + failsafe layer (see `failsafe.rs`) cover missed
//! reads without surfacing noise to the UI. The slow-device log
//! fires once per device per tick if any channel times out.
//!
//! Write permit: `poll_rate * MISSING_STATUS_THRESHOLD` (8), so
//! 8 s at the default tick. Set well above the steady-state read
//! window so a write is allowed to wait for a stuck reader to
//! finish or fail; if 8 s elapses the device is genuinely wedged
//! and the write is reported.
//!
//! See `main_loop.rs` for the full three-layer timing model
//! (`poll_rate`, `SNAPSHOT_TIMEOUT_MS`, this module's permit
//! timeouts) and how they relate.
//!
//! ## Per-device command delay
//!
//! Two call sites apply `apply_device_command_delay(delay_millis)`,
//! both inside the permit window so the next op observes the gap:
//!
//! - **Read path** — fired *once* per preload tick, after all
//!   per-channel reads complete, via
//!   `spawn_command_delay_holder`. The holder is a detached task
//!   that re-acquires the permit and holds it for `delay_millis`,
//!   queueing behind any in-flight per-channel acquires. Reads
//!   count as one batched "command" for the device's settle
//!   window — paying the delay N times per tick on an N-channel
//!   device would compound badly with the per-channel handoff,
//!   so we keep the original "once per preload" semantic.
//!
//! - **Write path** — fired *per write* inside the writer task.
//!   Each duty write to a channel pays its own settle window
//!   before the permit is released, since back-to-back fan
//!   writes on the same channel can otherwise overrun the
//!   device's command pacing.
//!
//! The default `delay_millis = 0` makes both call sites no-ops;
//! only manually-configured delays (rare, for misbehaving
//! hardware) actually pay this cost.
//!
//! ## Per-device write coalescer (`writers: HashMap<TypeIndex, Rc<WriterMailbox>>`)
//!
//! Fan-duty writes do NOT take the permit directly from the API
//! actor or the engine commanders. They enqueue into a
//! `WriterMailbox` (one per device): a `RefCell<HashMap<ChannelName, PendingWrite>>`
//! plus a `Notify`. A single per-device writer task spawned at
//! init drains the mailbox under the permit. Each iteration:
//!
//! 1. `select!` on `notify.notified()` / `shutdown.cancelled()`.
//! 2. Swap pending into a reused buffer (no per-tick allocation).
//! 3. For each `(channel, PendingWrite)`: take the permit (with
//!    the write-permit timeout), run the sysfs write, apply the
//!    per-device command delay, drop the permit, fan the result
//!    to every merged waiter via their `oneshot::Sender`s.
//!
//! `apply_setting_speed_fixed` validates, inserts-or-replaces the
//! pending entry for the channel, pushes its `oneshot::Sender`
//! onto the waiter list, signals `notify_one`, and awaits its
//! receiver. Newer writes to the same channel overwrite
//! `target_duty` and merge their senders into the existing
//! waiter list, so a burst collapses to one hardware write at the
//! latest value with all callers observing the same `Result`.
//!
//! Bound: `MAX_WAITERS_PER_PENDING_WRITE` (64). On overflow the
//! oldest sender is dropped with a "superseded" error so the
//! merged list stays bounded under any client behavior.
//!
//! ## Why coalescing only for fan-duty writes
//!
//! `apply_setting_speed_fixed` is the tick-driven hot path
//! (graph / mix / overlay commanders enqueue one write per
//! channel per tick). The other apply paths (`reset`,
//! `manual_control`, `speed_profile`) fire rarely and take the
//! permit directly. Caller-visible ordering across them is
//! preserved by the Semaphore's FIFO discipline: a caller that
//! awaits a direct-path apply before issuing a coalesced write
//! sees the direct-path operation observed by hardware first.
//!
//! ## Slow-device caching (read-side)
//!
//! At startup, `map_into_our_device_model` times the per-device
//! init extract block (fans + power + temps). When the wall clock
//! exceeds `device_read_permit_timeout`, the device is recorded
//! in `slow_devices` and gets a `duty_cache: HashMap<ChannelName,
//! DutyCacheEntry>`. The cache holds `last_known: Duty` and
//! `next_verify_at: Instant`. The preload's Fan branch checks the
//! cache: while `Instant::now() < next_verify_at`, it skips the
//! slow PWM read and reads only RPM via `read_one_fan_rpm_only`,
//! synthesizing the channel status from the cached duty. When
//! the verify window elapses (every `DUTY_CACHE_VERIFY_INTERVAL`,
//! default 30 s, staggered evenly per channel at init) it does a
//! real read and refreshes both fields. Fast devices skip this
//! path entirely.
//!
//! On every successful sysfs duty write, the writer task updates
//! `last_known` (but not `next_verify_at`); on `manual_control`,
//! `reset`, or `speed_profile` the entry is dropped via
//! `invalidate_duty_cache_entry` because duty control has just
//! been transferred to or from the device's own firmware.
//!
//! ## Write-skip via `preloaded_statuses`
//!
//! Before the writer task issues a sysfs duty write, it looks up
//! the channel's current duty in the shared `preloaded_statuses`
//! cache (already populated by every preload tick at init and on
//! each successful read). If the cached duty equals the target,
//! the writer fans `Ok(())` to all merged waiters and returns
//! without touching sysfs. This eliminates the 8 × ~475 ms of
//! redundant rewrites per tick under a steady-state graph
//! profile on the user's slow device, and incurs only a hash-map
//! lookup on fast devices. No extra sysfs reads — the
//! `preloaded_statuses` snapshot is at most one tick stale, and
//! engine commanders write at most one duty per channel per
//! tick, so there is no within-tick double-write that would make
//! that staleness observable.
//!
//! ## Round-robin channel start index
//!
//! `tick_count: Cell<u64>` increments once per `preload_statuses`
//! call. `preload_device_statuses` rotates the iteration starting
//! index by `tick_count % N_channels` independently for each of
//! the Power, Temp, and Fan passes. Removes the FIFO bias that
//! would otherwise cause later-iteration channels to
//! disproportionately time out under sustained slow-device
//! contention.
//!
//! ## Shutdown
//!
//! `HwmonRepo::shutdown` fires `shutdown_token.cancel()` first so
//! every per-device writer task observes its cancel arm, drains
//! its mailbox by signalling each waiter with a "cancelled" error,
//! and exits. The existing per-channel reset-to-default loop runs
//! after, taking the permit directly. Late callers reaching
//! `apply_setting_speed_fixed` after the cancel see
//! `is_cancelled()` and return early so no `oneshot::Sender` is
//! orphaned on a writer task that has already exited.

use crate::cc_fs;
use crate::config::Config;
use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelName, ChannelStatus, Device, DeviceInfo, DeviceType,
    DeviceUID, DriverInfo, DriverType, Duty, SpeedOptions, Status, Temp, TempInfo, TempName,
    TempStatus, TypeIndex, UID,
};
use crate::repositories::failsafe::{self, FailsafeStatusData, MISSING_STATUS_THRESHOLD};
use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
use crate::repositories::hwmon::devices::{DEVICE_NAMES_APPLE, HWMON_DEVICE_NAME_BLACKLIST};
use crate::repositories::hwmon::{auto_curve, devices, drivetemp, fans, power, temps, thinkpad};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::utils::apply_device_command_delay;
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bitflags::bitflags;
use heck::ToTitleCase;
use log::{debug, error, info, log, trace, warn};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::mem;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use strum::{Display, EnumString};
use tokio::sync::{oneshot, Notify, Semaphore, SemaphorePermit};
use tokio::time::{sleep, timeout, Instant};
use tokio_util::sync::CancellationToken;

/// Fraction of `poll_rate` a device preload is allowed before the
/// slow-device arm fires. This is the per-device "layer 3" budget
/// in the timing model documented at the top of `main_loop`; it is
/// independent of `SNAPSHOT_TIMEOUT_MS` and may exceed it. A read
/// above the snapshot budget just has its values appear in the
/// next snapshot; the per-channel staleness counter ticks while
/// the read is overdue, and the failsafe layer covers reads that
/// stay slow for `MISSING_STATUS_THRESHOLD` consecutive ticks.
///
/// Anchored so that at the minimum poll rate (0.5 s) the budget
/// reproduces the original 350 ms value, preserving historical
/// behavior on the fastest poll setting.
const READ_PERMIT_RATIO: f64 = 0.7;

/// Derives the read permit timeout from `poll_rate`. Pure helper so
/// the ratio is testable without constructing a full `HwmonRepo`.
fn device_read_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * READ_PERMIT_RATIO)
}

/// Derives the write permit timeout from `poll_rate`. Pure helper so
/// the formula is testable without constructing a full `HwmonRepo`.
/// `MISSING_STATUS_THRESHOLD` is a small `usize` (8) that fits within
/// `u8::MAX`, so the cast to `f64` is lossless.
#[allow(clippy::cast_precision_loss)]
fn device_write_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * MISSING_STATUS_THRESHOLD as f64)
}

/// Fraction of `poll_rate` allowed for the drivetemp ATA power-state
/// ioctl. Kept strictly below `READ_PERMIT_RATIO` so on timeout there
/// is still budget for the fallback temp read before the outer read
/// permit arm fires. Hardware-healthy ATA power-state checks complete
/// in milliseconds; any value >> that is a wedged controller.
const DRIVETEMP_IOCTL_RATIO: f64 = 0.4;

/// Derives the drivetemp ioctl timeout from `poll_rate`. Pure helper
/// so the ratio is testable without constructing a full `HwmonRepo`.
fn drivetemp_ioctl_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * DRIVETEMP_IOCTL_RATIO)
}

/// Wall-clock budget per `extract_*` call during
/// `map_into_our_device_model`. A normal hwmon device completes all
/// reads in tens of milliseconds; 5 s is a conservative ceiling that
/// still prevents a single wedged sysfs file from stalling daemon
/// startup indefinitely. On timeout the device is skipped and the
/// daemon proceeds with the remaining devices; the next restart will
/// re-attempt it. This is used per chanel group.
const INIT_EXTRACT_TIMEOUT: Duration = Duration::from_secs(5);

/// Cap on the `pwm_enable` write during suspend preparation. The
/// systemd/logind sleep notification normally arrives 2-5 s before the
/// machine actually suspends, so this must fit well inside that
/// budget. A healthy `ThinkPad` EC completes the writes in
/// microseconds; 1s is a generous ceiling for a slow EC
/// without risking the suspend deadline. On timeout we log and
/// move on; the fan stays in whatever mode it was in before the
/// write attempt.
const PREPARE_FOR_SLEEP_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Builds stub statuses (one per discovered channel) so the
/// failsafe seed includes every channel even if its first read
/// failed at init. Per-channel staleness can then track those
/// channels and substitute failsafe values once the missing-tick
/// threshold is exceeded; without this, a channel whose first
/// read failed would never appear in the failsafe map and so
/// would never surface a value to the UI.
///
/// Field presence (`Some` vs `None`) on the stub matches what the
/// streaming extractors would populate, because
/// `failsafe::create_failsafe_data` uses `.and(Some(MISSING_*))`
/// to gate which failsafe fields appear in the substituted value.
fn synthesize_initial_statuses(
    channels: &[HwmonChannelInfo],
) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
    let mut channel_stubs = Vec::with_capacity(channels.len());
    let mut temp_stubs = Vec::with_capacity(channels.len());
    for channel in channels {
        match channel.hwmon_type {
            HwmonChannelType::Fan => {
                channel_stubs.push(ChannelStatus {
                    name: channel.name.clone(),
                    rpm: channel.caps.has_rpm().then_some(0),
                    duty: channel.caps.has_pwm().then_some(0.0),
                    ..Default::default()
                });
            }
            HwmonChannelType::Power => {
                channel_stubs.push(ChannelStatus {
                    name: channel.name.clone(),
                    watts: Some(0.0),
                    ..Default::default()
                });
            }
            HwmonChannelType::Temp => {
                temp_stubs.push(TempStatus {
                    name: channel.name.clone(),
                    temp: 0.0,
                });
            }
            HwmonChannelType::Load | HwmonChannelType::Freq | HwmonChannelType::PowerCap => {
                // These channel types are not preloaded by hwmon,
                // so they have no failsafe entry to seed.
            }
        }
    }
    (channel_stubs, temp_stubs)
}

/// The `drivetemp` kernel module is non-standard and used for getting temps for HDDs. Part of its
/// implementation blocks temperature reads when the drive is spinning up which causes significant
/// read delays. Since this is pretty normal behavior for this driver, we handle it differently.
static DRIVETEMP: &str = "drivetemp";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum HwmonChannelType {
    Fan,
    Temp,
    Load,
    Freq,
    Power,
    PowerCap, // RAPL
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonChannelInfo {
    pub hwmon_type: HwmonChannelType,
    pub number: u8,
    pub pwm_enable_default: Option<u8>,
    pub name: String,
    pub label: Option<String>,
    pub auto_curve: AutoCurveInfo,
    pub caps: HwmonChannelCapabilities,
    // Paths that are often used are saved to avoid cloning
    pub pwm_path: Option<PathBuf>,
    pub rpm_path: Option<PathBuf>,
    pub temp_path: Option<PathBuf>,
}

impl Default for HwmonChannelInfo {
    fn default() -> Self {
        Self {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            pwm_enable_default: None,
            name: String::new(),
            label: None,
            auto_curve: AutoCurveInfo::None,
            caps: HwmonChannelCapabilities::empty(),
            pwm_path: None,
            rpm_path: None,
            temp_path: None,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HwmonChannelCapabilities: u32 {
        const FAN_WRITABLE = 1;
        const PWM = 1 << 1;
        const RPM = 1 << 2;
        const PWM_MODE = 1 << 3;
        // Specialities
        const APPLE_SMC = 1 << 15;
    }
}

impl HwmonChannelCapabilities {
    pub fn is_fan_controllable(&self) -> bool {
        self.contains(HwmonChannelCapabilities::FAN_WRITABLE)
    }

    pub fn has_pwm(&self) -> bool {
        self.contains(HwmonChannelCapabilities::PWM)
    }

    pub fn has_rpm(&self) -> bool {
        self.contains(HwmonChannelCapabilities::RPM)
    }

    pub fn has_pwm_mode(&self) -> bool {
        self.contains(HwmonChannelCapabilities::PWM_MODE)
    }

    pub fn is_apple_smc(&self) -> bool {
        self.contains(HwmonChannelCapabilities::APPLE_SMC)
    }

    pub fn is_non_controllable_rpm_fan(&self) -> bool {
        self.contains(HwmonChannelCapabilities::RPM)
            && !self.contains(HwmonChannelCapabilities::FAN_WRITABLE)
    }
}

/// Indicated support for hwmon auto curves (firmware profiles)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutoCurveInfo {
    None,
    PWM { point_length: u8 },
    Temp { temp_lengths: HashMap<TempName, u8> },
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct HwmonDriverInfo {
    pub name: String,
    pub path: PathBuf,
    pub model: Option<String>,
    pub u_id: UID,
    pub channels: Vec<HwmonChannelInfo>,
    /// this is used specifically for the `drivetemp` module,
    /// which has an associated block device path if found.
    pub block_dev_path: Option<PathBuf>,
    pub apple_smc: AppleMacSMC,
}

/// Hard cap on the number of waiters merged into a single `PendingWrite`.
/// A runaway client dragging a fan slider cannot grow this unboundedly;
/// on overflow the oldest waiter is dropped with a "superseded" error.
/// 64 is comfortably above any realistic burst from the engine
/// commanders (one tick per channel) and from a human-driven slider.
const MAX_WAITERS_PER_PENDING_WRITE: usize = 64;

/// Initial capacity for a `PendingWrite::waiters` Vec. The common
/// case is 1 (single user click, single tick from a commander) or
/// 2 (a write enqueued while another is in flight). The cap above
/// allows growth without realloc up to the bound; this just keeps
/// the steady-state allocation tiny.
const PENDING_WAITERS_INITIAL_CAPACITY: usize = 2;

/// Initial capacity for the writer mailbox's `pending` map and the
/// writer task's reuse buffer. Sized to fit a typical hwmon device's
/// full fan-channel set (most have 1-8) without growing. Bursts
/// beyond this still work; the map reallocates once and the larger
/// capacity is then reused on subsequent ticks.
const PENDING_INITIAL_CAPACITY: usize = 8;

const _: () = assert!(MAX_WAITERS_PER_PENDING_WRITE > 0);
const _: () = assert!(PENDING_WAITERS_INITIAL_CAPACITY <= MAX_WAITERS_PER_PENDING_WRITE);

/// A pending fan-duty write coalesced for one channel of one device.
/// Newer writes to the same channel replace `target_duty`; their
/// `oneshot::Sender` joins `waiters`. When the writer task drains
/// the entry it issues exactly one hardware write and fans the
/// `Result` to every waiter so all callers observe the same outcome.
struct PendingWrite {
    target_duty: Duty,
    waiters: Vec<oneshot::Sender<Result<(), String>>>,
}

/// Per-device coalescing inbox. The writer task owns processing;
/// `apply_setting_speed_fixed` only inserts into `pending` and
/// signals `notify`. `pending` is borrowed sync (no `.await` while
/// the borrow is live) so we can use `RefCell` on the single-
/// threaded runtime without a lock-across-await hazard.
struct WriterMailbox {
    pending: RefCell<HashMap<ChannelName, PendingWrite>>,
    notify: Notify,
}

/// Duty-cache entry for slow-device fan channels. `last_known` is
/// authoritative as long as we observed it within the last 30 s
/// (or just wrote it ourselves and the device is still in manual
/// mode); `next_verify_at` schedules the periodic real-read that
/// detects external drift.
struct DutyCacheEntry {
    last_known: Duty,
    next_verify_at: Instant,
}

/// Period between forced PWM duty re-reads on slow devices. Long
/// enough that the per-tick savings from skipping the read
/// dominate; short enough that an external sysfs edit (rare)
/// becomes visible within a minute. Verifications are staggered
/// across this window per channel so only ~one channel does a
/// real PWM read per tick on average.
const DUTY_CACHE_VERIFY_INTERVAL: Duration = Duration::from_secs(30);

const _: () = assert!(DUTY_CACHE_VERIFY_INTERVAL.as_secs() > 0);

/// A Repository for `HWMon` Devices
pub struct HwmonRepo {
    config: Rc<Config>,
    devices: HashMap<DeviceUID, (DeviceLock, Rc<HwmonDriverInfo>)>,
    /// Per-tick snapshot of every device's channel + temp readings.
    /// Wrapped in `Rc<RefCell<...>>` so the writer task can hold a
    /// clone and consult it for the write-skip path
    /// (`run_one_pending_write` looks up the channel's current duty
    /// here to decide whether the target sysfs write is a no-op).
    preloaded_statuses: Rc<RefCell<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>>,
    failsafe_statuses: RefCell<HashMap<TypeIndex, FailsafeStatusData>>,

    /// Permits for each `HWMon` device. This is useful for slower devices.
    /// `liqctld` already has an in-built device queue - where only one read or write
    /// request can be sent to the device at a time. This is that same idea but for hwmon devices.
    /// This also ensures that polling loops don't overlap and stack if the device hasn't finished
    /// responding from the previous polling loop.
    ///
    /// Stored as `Rc<Semaphore>` so the Semaphore can be cloned into
    /// a detached `spawn_local` task that re-acquires it across the
    /// command delay after a preload, without blocking the current
    /// preload's completion. The single-threaded runtime means `Rc`
    /// is sufficient; `acquire()` borrows the Semaphore through the
    /// task's async state machine (self-reference is fine because
    /// the task owns both the Rc and the permit).
    device_permits: HashMap<TypeIndex, Rc<Semaphore>>,

    /// Per-device write coalescer mailboxes. One writer task per
    /// device drains its mailbox under the device permit so a
    /// runaway tick stream cannot stack writes: only the latest
    /// target per channel survives, all waiters share its result.
    writers: HashMap<TypeIndex, Rc<WriterMailbox>>,

    /// Fired from `shutdown` so writer tasks exit before the reset
    /// loop runs. Pending waiters resolve with a "cancelled" error
    /// so callers do not hang on `rx.await` past daemon exit.
    shutdown_token: CancellationToken,

    /// Monotonic counter incremented once per `preload_statuses`
    /// call. Used to rotate the per-channel-type starting index in
    /// `preload_device_statuses` so no channel is permanently last
    /// in the FIFO under sustained contention with other ticks.
    tick_count: Cell<u64>,

    /// Devices whose initial extract block took longer than
    /// `device_read_permit_timeout`. The duty-cache + staggered
    /// 30 s verification path engages only for these; fast
    /// devices keep doing real reads every tick. Detected once at
    /// startup (in `map_into_our_device_model`) and immutable for
    /// the repo's lifetime — `poll_rate` is restart-only and the
    /// hardware doesn't get faster after boot.
    slow_devices: HashSet<TypeIndex>,

    /// Per-slow-device PWM duty cache, keyed by channel name. The
    /// preload path uses the cached duty (skipping the slow PWM
    /// read) until each channel's `next_verify_at` falls due, then
    /// it does a real read and refreshes both fields. Empty for
    /// fast devices. Held inside `Rc<RefCell<...>>` so the writer
    /// task can also update it on successful writes.
    duty_cache: HashMap<TypeIndex, Rc<RefCell<HashMap<ChannelName, DutyCacheEntry>>>>,

    /// Used to avoid logging a device-delay warning more than once and not on startup
    delay_logged: HashMap<TypeIndex, Cell<u8>>,

    /// Liquidctl driver `HWMon` paths, to be used to filter out duplicate `HWMon` devices
    lc_hwmon_paths: Vec<PathBuf>,

    /// Cached per-device command delay in milliseconds. Loaded at startup from config.
    device_delays: HashMap<DeviceUID, u16>,

    /// Snapshot of the read-permit timeout. `poll_rate` only changes on
    /// daemon restart, so this value is constant for the repo's lifetime
    /// and is computed once in `new` to avoid per-poll f64 math and a
    /// `RefCell` borrow on the config hot path.
    device_read_permit_timeout: Duration,

    /// Snapshot of the write-permit timeout. Constant for the repo's
    /// lifetime; see `device_read_permit_timeout`.
    device_write_permit_timeout: Duration,

    /// Snapshot of the drivetemp ioctl timeout. Constant for the
    /// repo's lifetime; bounds the `HDIO_DRIVE_CMD` ioctl that runs
    /// on the blocking pool during each preload tick.
    drivetemp_ioctl_timeout: Duration,
}

impl HwmonRepo {
    pub fn new(config: Rc<Config>, lc_locations: Vec<String>) -> Self {
        // `poll_rate` is captured at daemon startup and cannot change
        // without a restart, so the derived permit timeouts are frozen
        // here for the repo's lifetime.
        let poll_rate = config.get_settings().map(|s| s.poll_rate).unwrap_or(1.0);
        let device_read_permit_timeout = device_read_permit_timeout_for(poll_rate);
        let device_write_permit_timeout = device_write_permit_timeout_for(poll_rate);
        let drivetemp_ioctl_timeout = drivetemp_ioctl_timeout_for(poll_rate);
        Self {
            config,
            devices: HashMap::new(),
            preloaded_statuses: Rc::new(RefCell::new(HashMap::new())),
            failsafe_statuses: RefCell::new(HashMap::new()),
            device_permits: HashMap::new(),
            writers: HashMap::new(),
            shutdown_token: CancellationToken::new(),
            tick_count: Cell::new(0),
            slow_devices: HashSet::new(),
            duty_cache: HashMap::new(),
            delay_logged: HashMap::new(),
            lc_hwmon_paths: lc_locations
                .into_iter()
                .filter(|loc| loc.contains("hwmon/hwmon"))
                // blocking is fine during initialization:
                .filter_map(|loc| cc_fs::canonicalize(loc).ok())
                .collect(),
            device_delays: HashMap::new(),
            device_read_permit_timeout,
            device_write_permit_timeout,
            drivetemp_ioctl_timeout,
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

    /// Checks if the path matches a liquidctl device path.
    ///
    /// By default, `CoolerControl` will hide `HWMon` devices that are already detected
    /// by liquidctl. Liquidctl offers more features, like RGB & LCD control, that `HWMon`
    /// drivers don't.
    ///
    /// Liquidctl uses `HWMon` in their backend for many of their supported devices. This
    /// allows us to verify which one of the liquidctl devices have an exact path match to
    /// a `HWMon` device we've detected. The canonicalized path resolves the `HWMon` path
    /// to a very specific location in the system and device model, so false positives are
    /// near impossible.
    ///
    /// Additionally, liquidctl gives us a hidraw based `HWMon` path, and we use a `HWMon`
    /// class based path. Both of these paths are canonicalized to the same "real" path,
    /// negating any initial subsystem differences.
    fn path_matches_liquidctl_device(&self, base_path: &Path) -> bool {
        cc_fs::canonicalize(base_path).is_ok_and(|dev_path| self.lc_hwmon_paths.contains(&dev_path))
    }

    /// Maps driver infos to our Devices
    /// `ThinkPads` need special handling, see:
    /// [Kernel Docs](https://www.kernel.org/doc/html/latest/admin-guide/laptops/thinkpad-acpi.html#fan-control-and-monitoring-fan-speed-fan-enable-disable)
    ///
    /// `extract_timeout` bounds each initial status extraction so a
    /// wedged sysfs file cannot stall daemon startup. A device whose
    /// extraction times out is skipped; the daemon proceeds with the
    /// rest.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    async fn map_into_our_device_model(
        &mut self,
        hwmon_drivers: Vec<HwmonDriverInfo>,
        extract_timeout: Duration,
    ) -> Result<()> {
        debug_assert!(extract_timeout > Duration::ZERO);
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (index, driver) in hwmon_drivers.into_iter().enumerate() {
            let temps = driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: channel.label.as_ref().map_or_else(
                                || channel.name.to_title_case(),
                                |l| l.to_title_case(),
                            ),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut profile_max_length = 21; // Default
            let mut channels = HashMap::new();
            let mut thinkpad_fan_control = (
                driver.name == devices::DEVICE_NAME_THINK_PAD
                // first check if this is a ThinkPad
            )
                .then_some(false);
            for channel in &driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        if thinkpad_fan_control.is_some() && channel.number == 1 {
                            thinkpad_fan_control = Some(
                                // verify if fan control for this ThinkPad is enabled or not:
                                fans::set_pwm_enable(2, &driver.path, channel).await.is_ok(),
                            );
                        }
                        let extension = match &channel.auto_curve {
                            AutoCurveInfo::None => None,
                            AutoCurveInfo::PWM { point_length } => {
                                if point_length < &profile_max_length {
                                    profile_max_length = *point_length;
                                }
                                Some(ChannelExtensionNames::AutoHWCurve)
                            }
                            AutoCurveInfo::Temp { temp_lengths } => {
                                for point_length in temp_lengths.values() {
                                    if point_length < &profile_max_length {
                                        profile_max_length = *point_length;
                                    }
                                }
                                Some(ChannelExtensionNames::AutoHWCurve)
                            }
                        };
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            speed_options: Some(SpeedOptions {
                                fixed_enabled: channel
                                    .caps
                                    .contains(HwmonChannelCapabilities::FAN_WRITABLE),
                                extension,
                                ..Default::default()
                            }),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Power => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    _ => (), // other channel types are handled differently or don't have info
                }
            }
            let device_info = DeviceInfo {
                temps,
                channels,
                temp_min: 0,
                temp_max: 150,
                profile_max_length,
                model: driver.model.clone(),
                thinkpad_fan_control,
                driver_info: DriverInfo {
                    drv_type: DriverType::Kernel,
                    name: devices::get_device_driver_name(&driver.path).await,
                    version: sysinfo::System::kernel_version(),
                    locations: Self::get_driver_locations(&driver.path).await,
                },
                ..Default::default()
            };
            let type_index = (index + 1) as u8;
            // Measure the wall-clock cost of all initial sysfs
            // reads on this device. If the total exceeds the
            // device's read-permit budget, the device cannot keep
            // up with the configured poll rate and the slow-device
            // duty cache + staggered verification path engages for
            // it. Fast devices stay on the unconditional-real-read
            // path so the UI continues to show live values for
            // them.
            let extract_start = Instant::now();
            let Ok((mut channel_statuses, _)) =
                timeout(extract_timeout, fans::extract_fan_statuses(&driver)).await
            else {
                error!(
                    "Timed out after {extract_timeout:?} extracting initial fan statuses \
                     for hwmon device: {} — skipping device at init. Check that the hwmon \
                     sysfs files are responsive.",
                    driver.name
                );
                continue;
            };
            let Ok((power_statuses, _)) =
                timeout(extract_timeout, power::extract_power_status(&driver)).await
            else {
                error!(
                    "Timed out after {extract_timeout:?} extracting initial power statuses \
                     for hwmon device: {} — skipping device at init.",
                    driver.name
                );
                continue;
            };
            channel_statuses.extend(power_statuses);
            let Ok((temp_statuses, _)) =
                timeout(extract_timeout, temps::extract_temp_statuses(&driver)).await
            else {
                error!(
                    "Timed out after {extract_timeout:?} extracting initial temp statuses \
                     for hwmon device: {} — skipping device at init.",
                    driver.name
                );
                continue;
            };
            self.detect_slow_and_seed_duty_cache(
                type_index,
                extract_start.elapsed(),
                &driver.name,
                &channel_statuses,
            );
            // Failsafe seed comes from the discovered channel list,
            // not the extracted statuses, so a channel whose first
            // read failed at init is still tracked by per-channel
            // staleness. Without this, such a channel would never
            // surface to the UI even after the threshold elapses,
            // because the failsafe map would be missing its entry.
            let (failsafe_seed_channels, failsafe_seed_temps) =
                synthesize_initial_statuses(&driver.channels);
            let (channel_failsafes, temp_failsafes) =
                failsafe::create_failsafe_data(&failsafe_seed_channels, &failsafe_seed_temps);
            if let Some(fsd) = FailsafeStatusData::new(channel_failsafes, temp_failsafes) {
                self.failsafe_statuses.borrow_mut().insert(type_index, fsd);
            }
            self.preloaded_statuses.borrow_mut().insert(
                type_index,
                (channel_statuses.clone(), temp_statuses.clone()),
            );
            let mut device = Device::new(
                driver.name.clone(),
                DeviceType::Hwmon,
                type_index,
                None,
                device_info,
                Some(driver.u_id.clone()),
                poll_rate,
            );
            let status = Status {
                channels: channel_statuses,
                temps: temp_statuses,
                ..Default::default()
            };
            device.initialize_status_history_with(status, poll_rate);
            self.device_permits
                .insert(type_index, Rc::new(Semaphore::new(1)));
            self.writers.insert(
                type_index,
                Rc::new(WriterMailbox {
                    pending: RefCell::new(HashMap::with_capacity(PENDING_INITIAL_CAPACITY)),
                    notify: Notify::new(),
                }),
            );
            self.delay_logged.insert(type_index, Cell::new(0));
            self.devices.insert(
                device.uid.clone(),
                (Rc::new(RefCell::new(device)), Rc::new(driver)),
            );
        }
        Ok(())
    }

    /// Gets the info necessary to apply setting to the device channel
    fn get_hwmon_info(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<(&Rc<HwmonDriverInfo>, &HwmonChannelInfo, TypeIndex)> {
        let (device_lock, hwmon_driver) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        let channel_info = hwmon_driver
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| format!("Searching for channel name: {channel_name}"))?;
        Ok((hwmon_driver, channel_info, device_lock.borrow().type_index))
    }

    async fn get_driver_locations(base_path: &Path) -> Vec<String> {
        let hwmon_path = base_path.to_str().unwrap_or_default().to_owned();
        let device_path = devices::get_static_device_path_str(base_path);
        let mut locations = vec![hwmon_path, device_path.unwrap_or_default()];
        if let Some(mod_alias) = devices::get_device_mod_alias(base_path).await {
            locations.push(mod_alias);
        }
        if let Some(hid_phys) = devices::get_device_hid_phys(base_path).await {
            locations.push(hid_phys);
        }
        locations
    }

    /// Reads channel and temp statuses for one device and upserts them
    /// into the preloaded cache per channel as each read completes.
    /// Acquires the device permit *per channel* (not for the whole
    /// device) so writes enqueued mid-preload can interleave between
    /// channel reads instead of waiting for the entire device's
    /// channel set. Worst-case write latency drops from
    /// `N_channels * read_time` to ~one channel's read time.
    ///
    /// Fast channels on the device become visible to downstream the
    /// same tick they are read, even if a later channel on the same
    /// device is slow. Failing reads leave their cache entries
    /// untouched so downstream keeps seeing the last known good
    /// value, not a fabricated 0. Each upsert also flips a
    /// pre-allocated `fresh_this_tick` bool inside `FailsafeStatusData`
    /// so the snapshot timeout arm in `main_loop.rs` can recognize
    /// partial upserts as fresh instead of ticking every channel
    /// blindly. Staleness and failsafe substitution are handled per
    /// channel in `tick_staleness_and_log`, invoked at end-of-tick.
    ///
    /// Channel order is power → temp → fan, matching the historical
    /// "place quicker and more important channel extractions first"
    /// invariant from commit f02ed25d.
    async fn preload_device_statuses(&self, type_index: TypeIndex, driver: &Rc<HwmonDriverInfo>) {
        // Bail before doing any work if shutdown was already signalled
        // (via abort_pending or hwmon's own shutdown method). The main
        // loop's moro scope is waiting on us; staying here just delays
        // the daemon's shutdown sequence.
        if self.shutdown_token.is_cancelled() {
            return;
        }
        // Clear the fresh-this-tick flags at the start of this
        // preload attempt. The snapshot timeout arm reads the flags
        // as they get set by the per-channel upserts.
        // `is_failsafed` and `stale_ticks` persist across preloads.
        self.reset_fresh_this_tick(type_index);

        let drivetemp_suspended =
            drivetemp::is_suspended(driver.block_dev_path.as_ref(), self.drivetemp_ioctl_timeout)
                .await;
        let tick = self.tick_count.get();
        let mut any_permit_timeout = false;
        for ch_type in [
            HwmonChannelType::Power,
            HwmonChannelType::Temp,
            HwmonChannelType::Fan,
        ] {
            // Round-robin start index per channel type: the same
            // tick rotates Power, Temp, and Fan independently
            // because each type has its own channel count. This
            // removes the FIFO bias under sustained slow-device
            // contention (later-iteration channels were
            // disproportionately timing out every tick).
            let typed_channels: Vec<&HwmonChannelInfo> = driver
                .channels
                .iter()
                .filter(|c| c.hwmon_type == ch_type)
                .collect();
            let channel_count = typed_channels.len();
            if channel_count == 0 {
                continue;
            }
            // tick is a monotonic u64 wrapped at u64::MAX; the
            // truncation to usize on 32-bit targets is harmless
            // because both are reduced mod n where n is the
            // (small) channel count.
            #[allow(clippy::cast_possible_truncation)]
            let start = (tick as usize) % channel_count;
            for offset in 0..channel_count {
                // Check before each channel so a long permit-acquire
                // wait or a slow sysfs read does not stretch the tick
                // past the start of shutdown.
                if self.shutdown_token.is_cancelled() {
                    return;
                }
                let channel = typed_channels[(start + offset) % channel_count];
                let acquired = self
                    .read_one_channel(type_index, driver, channel, &ch_type, drivetemp_suspended)
                    .await;
                if acquired.not() {
                    any_permit_timeout = true;
                }
            }
        }

        // Single end-of-preload command delay: reads count as one
        // batched "command" for the device's settle window, matching
        // the original spawn_command_delay_holder semantic. The
        // holder re-acquires the permit and holds it for delay_ms,
        // so subsequent writes (and the next tick's preload) queue
        // behind it. Per-channel writes pay their own delay inside
        // the writer task; this only governs the read side.
        self.spawn_command_delay_holder(type_index, self.device_delay(&driver.u_id));

        if any_permit_timeout {
            self.log_slow_device(type_index, &driver.name);
        }
        self.tick_staleness_and_log(type_index, &driver.name);
    }

    /// Acquires the device permit with the read-permit timeout
    /// and runs the type-appropriate per-channel read, upserting
    /// the result. Returns `true` if the permit was acquired
    /// (regardless of read success), `false` if the acquire timed
    /// out. The per-device command delay is NOT applied here; it
    /// fires once at the end of `preload_device_statuses` so
    /// reads count as one batched "command" rather than paying
    /// the delay N times per tick.
    async fn read_one_channel(
        &self,
        type_index: TypeIndex,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
        ch_type: &HwmonChannelType,
        drivetemp_suspended: bool,
    ) -> bool {
        let semaphore = self.device_permits.get(&type_index).expect(
            "invariant: device_permits entry exists for every registered device type_index",
        );
        // The shutdown arm short-circuits a permit-waiting channel so
        // we do not burn the full device_read_permit_timeout before
        // the next iteration's bail check fires. Returning `true`
        // here avoids triggering log_slow_device for an aborted tick.
        let acquire = tokio::select! {
            () = self.shutdown_token.cancelled() => return true,
            () = sleep(self.device_read_permit_timeout) => None,
            permit = semaphore.acquire() => permit.ok(),
        };
        let Some(_permit) = acquire else {
            return false;
        };
        match ch_type {
            HwmonChannelType::Power => {
                if let Some(status) = power::read_one_power_status(driver, channel).await {
                    self.mark_channel_fresh(type_index, &status.name);
                    self.upsert_single_channel(type_index, status);
                }
            }
            HwmonChannelType::Temp => {
                let status = if drivetemp_suspended {
                    Some(drivetemp::default_suspended_temp_for(channel))
                } else {
                    temps::read_one_temp_status(driver, channel).await
                };
                if let Some(status) = status {
                    self.mark_temp_fresh(type_index, &status.name);
                    self.upsert_single_temp(type_index, status);
                }
            }
            HwmonChannelType::Fan => {
                let status = self.read_fan_channel(type_index, driver, channel).await;
                if let Some(status) = status {
                    self.mark_channel_fresh(type_index, &status.name);
                    self.upsert_single_channel(type_index, status);
                }
            }
            _ => {}
        }
        true
    }

    /// Slow-device-aware fan-channel read. For fast devices: real
    /// read of both PWM duty and RPM. For slow devices with a
    /// duty-cache entry whose `next_verify_at` is in the future:
    /// RPM-only read, synthesizing the `ChannelStatus` from the
    /// cached duty. For slow devices when the verify window has
    /// elapsed (or no cache entry exists yet): real read, then
    /// refresh the cache and reset `next_verify_at`.
    async fn read_fan_channel(
        &self,
        type_index: TypeIndex,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
        if self.slow_devices.contains(&type_index).not() {
            // Fast device: unchanged behavior.
            return if driver.apple_smc.detected {
                driver.apple_smc.read_one_fan_status(driver, channel).await
            } else {
                fans::read_one_fan_status(driver, channel).await
            };
        }
        let cache = self.duty_cache.get(&type_index);
        let cached_duty = cache.and_then(|c| {
            c.borrow()
                .get(&channel.name)
                .filter(|entry| Instant::now() < entry.next_verify_at)
                .map(|entry| entry.last_known)
        });
        if let Some(duty) = cached_duty {
            // Cache fresh: skip the slow PWM read.
            let rpm_result = if driver.apple_smc.detected {
                driver
                    .apple_smc
                    .read_one_fan_rpm_only(driver, channel)
                    .await
            } else {
                fans::read_one_fan_rpm_only(driver, channel).await
            };
            // `Option<Option<u32>>`: outer None means an expected
            // RPM read failed (omit the channel so failsafe
            // engages); inner None means no RPM cap.
            let rpm = rpm_result?;
            return Some(ChannelStatus {
                name: channel.name.clone(),
                rpm,
                duty: Some(f64::from(duty)),
                ..Default::default()
            });
        }
        // Verify due (or no cache entry): real read, refresh cache.
        let status = if driver.apple_smc.detected {
            driver.apple_smc.read_one_fan_status(driver, channel).await
        } else {
            fans::read_one_fan_status(driver, channel).await
        }?;
        if let Some(duty_f64) = status.duty {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let duty_u8 = duty_f64.round().clamp(0.0, 100.0) as Duty;
            if let Some(cache) = cache {
                cache.borrow_mut().insert(
                    channel.name.clone(),
                    DutyCacheEntry {
                        last_known: duty_u8,
                        next_verify_at: Instant::now() + DUTY_CACHE_VERIFY_INTERVAL,
                    },
                );
            }
        }
        Some(status)
    }

    /// Clears the `fresh_this_tick` flags for `type_index`. Called
    /// at the start of each preload attempt so the flags reflect
    /// only the in-flight attempt.
    fn reset_fresh_this_tick(&self, type_index: TypeIndex) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.reset_fresh_this_tick();
        }
    }

    /// Marks a channel as freshly upserted in the current preload
    /// attempt. The bool flip is keyed by pre-allocated name entries
    /// in `FailsafeStatusData`, so the hot path allocates nothing.
    fn mark_channel_fresh(&self, type_index: TypeIndex, name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.mark_channel_fresh(name);
        }
    }

    /// Mirror of `mark_channel_fresh` for temps.
    fn mark_temp_fresh(&self, type_index: TypeIndex, name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.mark_temp_fresh(name);
        }
    }

    /// Inserts one fresh channel status into the preloaded cache for
    /// `type_index`, replacing any prior entry with the same name or
    /// appending when absent. Short critical section: one `RefMut`
    /// borrow per channel, released before the next extractor yield.
    fn upsert_single_channel(&self, type_index: TypeIndex, fresh: ChannelStatus) {
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (channels, _) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let len_before = channels.len();
        if let Some(entry) = channels.iter_mut().find(|c| c.name == fresh.name) {
            *entry = fresh;
            debug_assert_eq!(channels.len(), len_before);
        } else {
            channels.push(fresh);
            debug_assert_eq!(channels.len(), len_before + 1);
        }
    }

    /// Mirror of `upsert_single_channel` for temp statuses.
    fn upsert_single_temp(&self, type_index: TypeIndex, fresh: TempStatus) {
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (_, temps) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let len_before = temps.len();
        if let Some(entry) = temps.iter_mut().find(|t| t.name == fresh.name) {
            *entry = fresh;
            debug_assert_eq!(temps.len(), len_before);
        } else {
            temps.push(fresh);
            debug_assert_eq!(temps.len(), len_before + 1);
        }
    }

    /// Calls `FailsafeStatusData::tick_per_channel_staleness` with
    /// the per-channel state already tracked inside `fsd`, and
    /// emits one-shot transition logs at the newly-failsafing and
    /// fully-recovered boundaries. Called from the end of
    /// `preload_device_statuses` and from the select! timeout arm.
    /// The timeout-arm caller sees whatever the currently running
    /// or most recently completed preload has upserted via
    /// `mark_channel_fresh` / `mark_temp_fresh`, so channels that
    /// already have real fresh values in the cache are not ticked.
    fn tick_staleness_and_log(&self, type_index: TypeIndex, driver_name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        let Some(fsd) = fsd_map.get_mut(&type_index) else {
            return;
        };
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (channels, temps) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let (newly_failsafing, just_recovered) = fsd.tick_per_channel_staleness(channels, temps);
        if newly_failsafing {
            error!(
                "Significant issue retrieving status for hwmon \
                 device: {driver_name}. Substituting failsafe \
                 values for stale channels."
            );
        }
        if just_recovered {
            info!(
                "Recovered from failsafe for hwmon device: {driver_name}. \
                 Resuming normal status reads."
            );
        }
    }

    /// Logging slow devices is triggered once the polling loop overlaps and the
    /// `DEVICE_READ_PERMIT_TIMEOUT` is reached.
    /// This only outputs a log on the 2nd occurrence, which then avoids outputting a log during
    /// initialization where some devices are under extra load, but makes sure to log it if it
    /// happens during normal polling loop operations.
    fn log_slow_device(&self, type_index: TypeIndex, driver_name: &str) {
        // Invariant: every type_index in `self.devices` has a
        // matching `delay_logged` entry. Both maps are populated
        // together in `map_into_our_device_model` and never removed
        // for the repo's lifetime, so a missing entry here means
        // the invariant was broken by a refactor.
        let slot = self
            .delay_logged
            .get(&type_index)
            .expect("invariant: delay_logged entry exists for every registered device type_index");
        let slow_device_trigger_count = slot.get();
        if slow_device_trigger_count > 1 {
            return;
        }
        if slow_device_trigger_count == 1 {
            let log_level = if driver_name == DRIVETEMP {
                log::Level::Debug
            } else {
                log::Level::Warn
            };
            log!(
                log_level,
                "Slow HWMon Device detected for: {driver_name}. \
                This device may be slow to update and respond."
            );
        }
        slot.replace(slow_device_trigger_count + 1);
    }

    async fn get_permit_with_write_timeout(
        &self,
        type_index: TypeIndex,
        driver_name: &str,
        channel_name: &str,
    ) -> Result<SemaphorePermit<'_>> {
        tokio::select! {
            () = sleep(self.device_write_permit_timeout) => Err(anyhow!(
                "TIMEOUT HWMon device: {driver_name} channel: {channel_name}; waiting to apply \
                fan speed. There will be significant issues handling this device due to extreme lag."
            )),
            device_permit = self.device_permits
                .get(&type_index)
                .expect("invariant: device_permits entry exists for every registered device type_index")
                .acquire() => device_permit.map_err(|e| anyhow!(e)),
        }
    }

    /// Removes one channel's slow-device duty-cache entry. Called
    /// from any apply path that hands duty control to (or back
    /// from) the device's own firmware: `manual_control` to enter
    /// manual mode, `reset` to return to auto, `speed_profile` to
    /// install a hardware curve. After invalidation the next
    /// preload re-reads the real PWM duty and reseeds the cache.
    /// No-op for fast devices (no entry to remove).
    fn invalidate_duty_cache_entry(&self, type_index: TypeIndex, channel_name: &str) {
        let Some(cache) = self.duty_cache.get(&type_index) else {
            return;
        };
        cache.borrow_mut().remove(channel_name);
    }

    /// If the just-completed init extract block ran longer than
    /// the device's read-permit budget, mark the device as slow
    /// and seed its duty cache from the initial fan reads. The
    /// cache's `next_verify_at` values are staggered evenly across
    /// `DUTY_CACHE_VERIFY_INTERVAL` so verification reads arrive
    /// one-channel-at-a-time rather than all at once. No-op for
    /// fast devices.
    fn detect_slow_and_seed_duty_cache(
        &mut self,
        type_index: TypeIndex,
        extract_elapsed: Duration,
        driver_name: &str,
        channel_statuses: &[ChannelStatus],
    ) {
        if (extract_elapsed > self.device_read_permit_timeout).not() {
            return;
        }
        info!(
            "Slow HWMon device detected: {driver_name} took {extract_elapsed:?} for initial \
             reads (budget {:?}); enabling duty cache.",
            self.device_read_permit_timeout
        );
        self.slow_devices.insert(type_index);
        let fan_count = channel_statuses
            .iter()
            .filter(|s| s.duty.is_some() || s.rpm.is_some())
            .count()
            .max(1);
        let mut entries = HashMap::with_capacity(channel_statuses.len());
        let now = Instant::now();
        let mut fan_index = 0_u32;
        #[allow(clippy::cast_precision_loss)]
        let fan_count_f64 = fan_count as f64;
        for status in channel_statuses {
            let Some(duty) = status.duty else {
                continue;
            };
            fan_index += 1;
            let stagger = DUTY_CACHE_VERIFY_INTERVAL.mul_f64(f64::from(fan_index) / fan_count_f64);
            // ChannelStatus.duty is f64 (0..=100 from
            // pwm_value_to_duty); writer task targets are Duty
            // (u8). Round to u8 here so the write-skip path can
            // compare apples-to-apples.
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let duty_u8 = duty.round().clamp(0.0, 100.0) as Duty;
            entries.insert(
                status.name.clone(),
                DutyCacheEntry {
                    last_known: duty_u8,
                    next_verify_at: now + stagger,
                },
            );
        }
        self.duty_cache
            .insert(type_index, Rc::new(RefCell::new(entries)));
    }

    /// Spawns a detached task that re-acquires the device permit
    /// and holds it for the command delay. No-op when
    /// `delay_millis == 0`. Called once at the end of
    /// `preload_device_statuses` so reads count as one batched
    /// "command" for the device's settle window: a write or the
    /// next tick's preload that arrives during the reads queues
    /// behind the holder in the Semaphore's FIFO waiter list and
    /// observes the configured gap before its own acquire wins.
    fn spawn_command_delay_holder(&self, type_index: TypeIndex, delay_millis: u16) {
        if delay_millis == 0 {
            return;
        }
        let Some(permit) = self.device_permits.get(&type_index) else {
            return;
        };
        let permit = Rc::clone(permit);
        tokio::task::spawn_local(async move {
            // The permit borrows from `permit` through this .await.
            // The async state machine stores both the Rc and the
            // SemaphorePermit, so the self-reference is sound for
            // the life of this task.
            let Ok(_held) = permit.acquire().await else {
                // Semaphore closed; daemon is shutting down. Drop
                // silently and let the runtime tear down.
                return;
            };
            apply_device_command_delay(delay_millis).await;
            // `_held` is dropped here, releasing the permit.
        });
    }

    /// Spawns one writer task per registered hwmon device. Called
    /// from `initialize_devices` after `load_device_delays` so each
    /// task captures the resolved per-device command delay. The task
    /// owns Rc clones of the mailbox, semaphore, driver, and config,
    /// so it stays alive for the daemon's lifetime even though
    /// `HwmonRepo::new` does not detain the spawned future.
    fn spawn_writer_tasks(&self) {
        let write_permit_timeout = self.device_write_permit_timeout;
        for (device_uid, (device_lock, driver)) in &self.devices {
            let type_index = device_lock.borrow().type_index;
            let Some(mailbox) = self.writers.get(&type_index) else {
                continue;
            };
            let Some(semaphore) = self.device_permits.get(&type_index) else {
                continue;
            };
            let task = WriterTask {
                type_index,
                mailbox: Rc::clone(mailbox),
                semaphore: Rc::clone(semaphore),
                driver: Rc::clone(driver),
                config: Rc::clone(&self.config),
                preloaded_statuses: Rc::clone(&self.preloaded_statuses),
                duty_cache: self.duty_cache.get(&type_index).map(Rc::clone),
                shutdown: self.shutdown_token.clone(),
                write_permit_timeout,
                delay_millis: self.device_delay(device_uid),
            };
            tokio::task::spawn_local(run_writer_task(task));
        }
    }
}

/// Per-device parameters captured by `run_writer_task`. Bundled to
/// keep the spawn site readable; lives only as long as one task.
struct WriterTask {
    type_index: TypeIndex,
    mailbox: Rc<WriterMailbox>,
    semaphore: Rc<Semaphore>,
    driver: Rc<HwmonDriverInfo>,
    config: Rc<Config>,
    /// Shared with `HwmonRepo` so the writer can short-circuit a
    /// sysfs write when the target equals the channel's current
    /// known duty. Read-only from the writer task's perspective.
    preloaded_statuses: Rc<RefCell<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>>,
    /// `Some` only for slow devices: the writer updates
    /// `last_known` on every successful sysfs write so the next
    /// preload tick can use the cached value rather than reading
    /// the slow PWM file. `next_verify_at` is left untouched on
    /// writes — it advances only on real verification reads.
    duty_cache: Option<Rc<RefCell<HashMap<ChannelName, DutyCacheEntry>>>>,
    shutdown: CancellationToken,
    write_permit_timeout: Duration,
    delay_millis: u16,
}

/// Drains the writer mailbox under the device permit until the
/// shutdown token fires. Coalescing happens at the enqueue side:
/// here we only see the latest target per channel and the merged
/// list of waiters. Each iteration swaps the mailbox's `pending`
/// with a reused empty buffer so writes that arrive during a drain
/// run on the next iteration, never silently merged into an
/// in-flight operation, and we do not allocate a fresh map every
/// tick.
async fn run_writer_task(task: WriterTask) {
    let mut buffer: HashMap<ChannelName, PendingWrite> =
        HashMap::with_capacity(PENDING_INITIAL_CAPACITY);
    loop {
        tokio::select! {
            () = task.mailbox.notify.notified() => {}
            () = task.shutdown.cancelled() => {
                cancel_remaining_waiters(&task.mailbox);
                return;
            }
        }
        debug_assert!(buffer.is_empty(), "buffer must start each iteration empty");
        swap_pending_into(&task.mailbox, &mut buffer);
        for (channel_name, pending) in buffer.drain() {
            run_one_pending_write(&task, channel_name, pending).await;
        }
    }
}

fn swap_pending_into(mailbox: &WriterMailbox, buffer: &mut HashMap<ChannelName, PendingWrite>) {
    mem::swap(&mut *mailbox.pending.borrow_mut(), buffer);
}

fn drain_pending(mailbox: &WriterMailbox) -> HashMap<ChannelName, PendingWrite> {
    mem::take(&mut *mailbox.pending.borrow_mut())
}

/// Inserts the new target into `pending`, merging waiters if a write
/// for `channel_name` is already queued. Returns the `oneshot::Receiver`
/// the caller awaits. The borrow of `pending` is released before the
/// caller awaits, so this is safe to invoke from inside an async fn.
/// On overflow (>= `MAX_WAITERS_PER_PENDING_WRITE` already queued),
/// the oldest waiter is dropped with a "superseded by newer write"
/// error so the merged list stays bounded.
fn enqueue_pending_write(
    mailbox: &Rc<WriterMailbox>,
    channel_name: &str,
    target_duty: Duty,
) -> oneshot::Receiver<Result<(), String>> {
    debug_assert!(target_duty <= 100, "caller must validate target_duty");
    let (tx, rx) = oneshot::channel();
    let mut pending = mailbox.pending.borrow_mut();
    let entry = pending
        .entry(channel_name.to_string())
        .or_insert_with(|| PendingWrite {
            target_duty,
            waiters: Vec::with_capacity(PENDING_WAITERS_INITIAL_CAPACITY),
        });
    entry.target_duty = target_duty;
    if entry.waiters.len() >= MAX_WAITERS_PER_PENDING_WRITE {
        // Drop the oldest sender. The user-visible final state still
        // reflects the latest target via the surviving waiters; only
        // the dropped caller's ack is lost.
        let oldest = entry.waiters.remove(0);
        let _ = oldest.send(Err(
            "HWMon write superseded by newer write (waiter list overflow)".to_string(),
        ));
    }
    debug_assert!(entry.waiters.len() < MAX_WAITERS_PER_PENDING_WRITE);
    entry.waiters.push(tx);
    debug_assert!(entry.waiters.is_empty().not());
    rx
}

/// Drains the mailbox and resolves every waiter with a cancelled
/// error. Sync so it never re-enters the runtime: callers run it
/// inside the writer task's shutdown branch where no `.await` is
/// allowed across the `pending` borrow.
fn cancel_remaining_waiters(mailbox: &WriterMailbox) {
    let entries = drain_pending(mailbox);
    for (_, pending) in entries {
        fan_out_error(
            pending.waiters,
            "HWMon writer cancelled: daemon shutting down",
        );
    }
}

async fn run_one_pending_write(
    task: &WriterTask,
    channel_name: ChannelName,
    pending: PendingWrite,
) {
    debug_assert!(
        pending.target_duty <= 100,
        "enqueue_pending_write validates"
    );
    debug_assert!(
        pending.waiters.is_empty().not(),
        "drained entries always carry at least one waiter"
    );
    // Write-skip: if the channel's most recent known duty equals
    // the target, the sysfs write is a no-op. Resolve waiters
    // immediately and avoid the expensive write (e.g. ~475 ms per
    // channel on the slow device that motivated this path).
    if current_duty_matches_target(task, &channel_name, pending.target_duty) {
        fan_out_result(pending.waiters, &Ok(()));
        return;
    }
    let driver_name = task.driver.name.as_str();
    let acquire: Result<SemaphorePermit<'_>, String> = tokio::select! {
        () = task.shutdown.cancelled() => Err(format!(
            "HWMon write cancelled for {driver_name}:{channel_name}: daemon shutting down"
        )),
        () = sleep(task.write_permit_timeout) => Err(format!(
            "TIMEOUT HWMon device: {driver_name} channel: {channel_name}; waiting to apply \
             fan speed. There will be significant issues handling this device due to extreme lag."
        )),
        permit = task.semaphore.acquire() => permit.map_err(|e| format!(
            "HWMon write failed for {driver_name}:{channel_name}: semaphore closed ({e})"
        )),
    };
    let permit = match acquire {
        Ok(permit) => permit,
        Err(message) => {
            fan_out_error(pending.waiters, &message);
            return;
        }
    };
    debug!(
        "Applying HWMON device: {driver_name} channel: {channel_name}; \
         Fixed Speed: {}",
        pending.target_duty
    );
    let result = apply_pwm_duty_write(
        &task.config,
        &task.driver,
        &channel_name,
        pending.target_duty,
    )
    .await
    .map_err(|err| err.to_string());
    apply_device_command_delay(task.delay_millis).await;
    drop(permit);
    if result.is_ok() {
        // Slow-device duty cache absorbs the write so the next
        // preload tick can serve `last_known` without doing the
        // slow PWM read. `next_verify_at` is unchanged: writes
        // do not reset the verification clock.
        if let Some(cache) = task.duty_cache.as_ref() {
            let mut entries = cache.borrow_mut();
            entries
                .entry(channel_name.clone())
                .and_modify(|e| e.last_known = pending.target_duty)
                .or_insert_with(|| DutyCacheEntry {
                    last_known: pending.target_duty,
                    next_verify_at: Instant::now() + DUTY_CACHE_VERIFY_INTERVAL,
                });
        }
    }
    fan_out_result(pending.waiters, &result);
}

/// Looks up the channel's current duty in `preloaded_statuses` and
/// returns true when it matches `target_duty`. Returning true short-
/// circuits the sysfs write. Falls back to false (i.e. proceed with
/// the write) when no entry exists for the channel — the cache is
/// populated from init reads, so an absent entry means we genuinely
/// don't know the device state.
fn current_duty_matches_target(task: &WriterTask, channel_name: &str, target_duty: Duty) -> bool {
    debug_assert!(target_duty <= 100, "caller must validate target_duty");
    let map = task.preloaded_statuses.borrow();
    let Some((channels, _)) = map.get(&task.type_index) else {
        return false;
    };
    let Some(status) = channels.iter().find(|c| c.name == channel_name) else {
        return false;
    };
    let Some(current_duty) = status.duty else {
        return false;
    };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let current_u8 = current_duty.round().clamp(0.0, 100.0) as Duty;
    debug_assert!(current_u8 <= 100, "ChannelStatus.duty must be 0..=100");
    current_u8 == target_duty
}

fn fan_out_error(waiters: Vec<oneshot::Sender<Result<(), String>>>, message: &str) {
    for waiter in waiters {
        let _ = waiter.send(Err(message.to_string()));
    }
}

fn fan_out_result(waiters: Vec<oneshot::Sender<Result<(), String>>>, result: &Result<(), String>) {
    for waiter in waiters {
        let _ = waiter.send(result.clone());
    }
}

/// Runs the actual sysfs fan-duty write. Mirrors the per-driver
/// branching that `apply_setting_speed_fixed` used pre-coalescer,
/// so behavior on `ThinkPad` / Apple SMC / generic hwmon is identical.
async fn apply_pwm_duty_write(
    config: &Rc<Config>,
    driver: &Rc<HwmonDriverInfo>,
    channel_name: &str,
    target_duty: Duty,
) -> Result<()> {
    let Some(channel_info) = driver
        .channels
        .iter()
        .find(|c| c.hwmon_type == HwmonChannelType::Fan && c.name == channel_name)
    else {
        return Err(anyhow!(
            "Channel not found while writing HWMon {}:{channel_name}",
            driver.name
        ));
    };
    if driver.name == devices::DEVICE_NAME_THINK_PAD {
        thinkpad::apply_speed_fixed(config, driver, channel_info, target_duty).await
    } else if driver.apple_smc.detected {
        driver
            .apple_smc
            .set_fan_duty(channel_info.number, target_duty)
            .await
    } else {
        fans::set_pwm_duty(&driver.path, channel_info, target_duty)
            .await
            .map_err(|err| {
                anyhow!(
                    "Error on {}:{channel_name} for duty {target_duty} - {err}",
                    driver.name
                )
            })
    }
}

#[async_trait(?Send)]
impl Repository for HwmonRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Hwmon
    }

    #[allow(clippy::too_many_lines)]
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        let base_paths = devices::find_all_hwmon_device_paths();
        if base_paths.is_empty() {
            info!(
                "No HWMon devices were found, try installing lm-sensors and running sensors-detect"
            );
            return Ok(());
        }
        debug!("Detected HWMon device paths: {base_paths:?}");
        let mut hwmon_drivers: Vec<HwmonDriverInfo> = Vec::new();
        let settings = self.config.get_settings()?;
        for path in base_paths {
            debug!("Processing HWMon device path: {}", path.display());
            let device_name = devices::get_device_name(&path).await;
            debug!("Detected Device Name: {device_name}");
            if HWMON_DEVICE_NAME_BLACKLIST.contains(&device_name.trim()) {
                continue;
            }
            if settings.hide_duplicate_devices && self.path_matches_liquidctl_device(&path) {
                info!(
                    "Skipping HWMon detected device: {device_name} due to an existing \
                    duplicate liquidctl device"
                );
                continue;
            }
            let u_id = devices::get_device_unique_id(&path, &device_name).await;
            debug!("Detected UID: {u_id}");
            let device_uid =
                Device::create_uid_from(&device_name, DeviceType::Hwmon, 0, Some(&u_id));
            let cc_device_setting = self
                .config
                .get_cc_settings_for_device(&device_uid)
                .ok()
                .flatten();
            if cc_device_setting.as_ref().is_some_and(|s| s.disable) {
                info!("Skipping disabled device: {device_name} with UID: {device_uid}");
                continue;
            }
            let disabled_channels =
                cc_device_setting.map_or_else(Vec::new, |setting| setting.get_disabled_channels());
            let mut channels = vec![];
            if DEVICE_NAMES_APPLE.contains(&device_name.as_str()) {
                AppleMacSMC::init_fans(&path, &mut channels, &disabled_channels).await;
            } else {
                match fans::init_fans(&path, &device_name).await {
                    Ok(fans) => channels.extend(
                        fans.into_iter()
                            .filter(|fan| disabled_channels.contains(&fan.name).not())
                            .collect::<Vec<HwmonChannelInfo>>(),
                    ),
                    Err(err) => error!("Error initializing Hwmon Fans: {err}"),
                }
            }
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(
                    temps
                        .into_iter()
                        .filter(|temp| disabled_channels.contains(&temp.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing Hwmon Temps: {err}"),
            }
            match power::init_power(&path).await {
                Ok(power) => channels.extend(
                    power
                        .into_iter()
                        .filter(|power| disabled_channels.contains(&power.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing Hwmon Power: {err}"),
            }
            if channels.is_empty() {
                debug!(
                    "No fans, temps, or power detected under {}, skipping.",
                    path.display()
                );
                continue;
            }
            let block_dev_path = if device_name == DRIVETEMP && settings.drivetemp_suspend {
                drivetemp::get_verified_block_device_path(&path)
                    .inspect_err(|err| warn!("Error getting block device path: {err}"))
                    .ok()
            } else {
                None
            };
            let apple_smc = if DEVICE_NAMES_APPLE.contains(&device_name.as_str()) {
                AppleMacSMC::new(&path, &channels, &device_name).await
            } else {
                AppleMacSMC::not_applicable()
            };
            let pci_device_names = devices::get_device_pci_names(&path).await;
            let model = devices::get_device_model_name(&path).await.or_else(|| {
                pci_device_names.and_then(|names| names.subdevice_name.or(names.device_name))
            });
            debug!("Detected Device Model: {model:?}");
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                u_id,
                channels,
                block_dev_path,
                apple_smc,
            };
            hwmon_drivers.push(hwmon_driver_info);
        }
        devices::handle_duplicate_device_names(&mut hwmon_drivers).await;
        // re-sorted by name to help keep some semblance of order after reboots & device changes.
        hwmon_drivers.sort_by(|d1, d2| d1.name.cmp(&d2.name));

        self.map_into_our_device_model(hwmon_drivers, INIT_EXTRACT_TIMEOUT)
            .await?;
        self.load_device_delays();
        self.spawn_writer_tasks();

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(uid.clone(), (device.borrow().clone(), hwmon_info.clone()));
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Hwmon Devices: {init_devices:?}");
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
                "Initialized Hwmon Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
            );
        }
        trace!(
            "Time taken to initialize all Hwmon devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("HWMON Repository initialized");
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
        // Bump once per tick so per-device round-robin start
        // indices in preload_device_statuses advance together;
        // wraps at u64::MAX which is unreachable in practice.
        self.tick_count.set(self.tick_count.get().wrapping_add(1));
        moro_local::async_scope!(|scope| {
            for (device_lock, driver) in self.devices.values() {
                let type_index = device_lock.borrow().type_index;
                let self = Rc::clone(&self);
                scope.spawn(async move {
                    // The device permit is now acquired per channel
                    // inside preload_device_statuses so the writer
                    // task can interleave between channel reads on
                    // a slow device. Slow-device logging and per-
                    // channel staleness ticking happen inside the
                    // per-channel loop, so no outer timeout arm is
                    // needed here.
                    self.preload_device_statuses(type_index, driver).await;
                });
            }
        })
        .await;
        trace!(
            "STATUS PRELOAD Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        for (device, _) in self.devices.values() {
            let preloaded_statuses_map = self.preloaded_statuses.borrow();
            let device_index = device.borrow().type_index;
            let preloaded_statuses = preloaded_statuses_map.get(&device_index);
            let Some((channels, temps)) = preloaded_statuses.cloned() else {
                error!("There is no status preloaded for this device: {device_index}");
                continue;
            };
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!(
                "Hwmon device: {} status was updated with: {status:?}",
                device.borrow().name
            );
            device.borrow_mut().set_status(status);
        }
        Ok(())
    }

    async fn abort_pending(&self) {
        // Cancel as soon as the main loop notices shutdown so spawned
        // preload tasks self-bail and writer tasks exit their
        // notified-await, freeing device permits before the moro scope
        // drops. Without this, a slow device's in-flight preload kept
        // the scope alive long enough for liqctld's watch_child grace
        // window to fire and force-kill the python service before
        // liquidctl_repo::shutdown could /quit it cleanly.
        // CancellationToken::cancel is idempotent, so the duplicate
        // call inside `shutdown` below is harmless.
        self.shutdown_token.cancel();
    }

    async fn shutdown(&self) -> Result<()> {
        // Stop accepting new coalesced writes and unblock any pending
        // ones before the reset loop runs. Without this, a wedged
        // device's writer task still holding the permit would force
        // every reset write to wait the full write-permit timeout.
        self.shutdown_token.cancel();
        // Continue-on-error: a permit timeout or write failure on one
        // channel must not skip the remaining channels. Leaving later
        // fans stuck in manual mode after shutdown is worse than the
        // cost of logging every failure and reporting an aggregated
        // error at the end.
        let mut failures: Vec<String> = Vec::new();
        for (device_uid, (device_lock, hwmon_driver)) in &self.devices {
            let type_index = device_lock.borrow().type_index;
            for channel_info in &hwmon_driver.channels {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                debug!(
                    "Applying HWMON device: {device_uid} channel: {}; \
                    Resetting to Original fan control mode",
                    channel_info.name
                );
                let device_permit = match self
                    .get_permit_with_write_timeout(
                        type_index,
                        &hwmon_driver.name,
                        &channel_info.name,
                    )
                    .await
                {
                    Ok(permit) => permit,
                    Err(err) => {
                        error!(
                            "Shutdown reset skipped for {}:{} - permit timeout: {err}",
                            hwmon_driver.name, channel_info.name
                        );
                        failures.push(format!("{}:{}", hwmon_driver.name, channel_info.name));
                        continue;
                    }
                };
                if let Err(err) =
                    fans::set_pwm_enable_to_default_or_auto(&hwmon_driver.path, channel_info).await
                {
                    error!(
                        "Shutdown reset failed for {}:{}: {err}",
                        hwmon_driver.name, channel_info.name
                    );
                    failures.push(format!("{}:{}", hwmon_driver.name, channel_info.name));
                }
                drop(device_permit);
            }
        }
        if failures.is_empty() {
            info!("HWMON Repository shutdown");
            Ok(())
        } else {
            Err(anyhow!(
                "HWMON Repository shutdown completed with {} channel failure(s): {}",
                failures.len(),
                failures.join(", ")
            ))
        }
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let (hwmon_driver, channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        debug!(
            "Applying HWMON device: {device_uid} channel: {channel_name}; Resetting to Original fan control mode"
        );
        // Reset hands duty control back to the device's auto
        // mode, so the cached "last_known" no longer reflects
        // reality. Drop the entry so the next preload reseeds it
        // from a real read.
        self.invalidate_duty_cache_entry(type_index, channel_name);
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        let result = if hwmon_driver.apple_smc.detected {
            hwmon_driver
                .apple_smc
                .set_to_auto_control(channel_info.number)
                .await
        } else {
            fans::set_pwm_enable_to_default_or_auto(&hwmon_driver.path, channel_info).await
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_manual_control(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<()> {
        let (hwmon_driver, channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        // Entering manual mode: until our first duty write the
        // device may be sitting at whatever value the kernel /
        // firmware had it at. Drop any stale cache entry so the
        // next preload reads the real value.
        self.invalidate_duty_cache_entry(type_index, channel_name);
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        let result = if hwmon_driver.apple_smc.detected {
            hwmon_driver
                .apple_smc
                .set_to_manual_control(channel_info.number)
                .await
        } else {
            fans::set_pwm_enable(
                fans::PWM_ENABLE_MANUAL_VALUE,
                &hwmon_driver.path,
                channel_info,
            )
            .await
            .map_err(|err| {
                anyhow!(
                    "Error on {}:{channel_name} for Manual Control - {err}",
                    hwmon_driver.name
                )
            })
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: Duty,
    ) -> Result<()> {
        // Validate before any state mutation. Out-of-range duty must
        // never reach the coalescer pending map.
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {speed_fixed}"));
        }
        // Reject after shutdown so a late call cannot orphan a
        // sender on a writer task that has already exited.
        if self.shutdown_token.is_cancelled() {
            return Err(anyhow!(
                "HWMon writer cancelled: daemon shutting down ({device_uid}:{channel_name})"
            ));
        }
        // get_hwmon_info also asserts the channel exists on this
        // device; the channel_info itself is looked up again inside
        // the writer task at write time.
        let (hwmon_driver, _, type_index) = self.get_hwmon_info(device_uid, channel_name)?;
        let mailbox = self.writers.get(&type_index).with_context(|| {
            format!(
                "No writer mailbox for HWMon device {} (type_index {type_index})",
                hwmon_driver.name
            )
        })?;
        let rx = enqueue_pending_write(mailbox, channel_name, speed_fixed);
        mailbox.notify.notify_one();
        match rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(anyhow!(message)),
            Err(_recv) => Err(anyhow!(
                "HWMon writer for {}:{channel_name} no longer running",
                hwmon_driver.name
            )),
        }
    }

    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(Temp, Duty)],
    ) -> Result<()> {
        let (hwmon_driver, fan_channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        if fan_channel_info.auto_curve == AutoCurveInfo::None {
            return Err(anyhow!(
                "Applying Internal Profile Error: device_uid: {device_uid} channel: {channel_name} does not support auto curves."
            ));
        }
        if &temp_source.device_uid != device_uid {
            return Err(anyhow!(
                "Applying Internal Profile Error: temp_source device_uid: {} does not match this device. \
                Auto curves temperature sources must be internal to the device.",
                temp_source.device_uid
            ));
        }
        let temp_channel_info = hwmon_driver
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Temp
                    && channel.name == temp_source.temp_name
            })
            .with_context(|| {
                format!("Searching for temp channel name: {}", temp_source.temp_name)
            })?;
        // Hardware auto curves take over duty control: the cached
        // "last_known" no longer reflects what the device is
        // actually outputting. Drop the entry so the next preload
        // reseeds from a real read.
        self.invalidate_duty_cache_entry(type_index, channel_name);
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        debug!(
            "Applying HWMON device: {device_uid} channel: {channel_name}; Speed Profile: {speed_profile:?}"
        );
        let result = auto_curve::apply_curve(
            &hwmon_driver.path,
            fan_channel_info,
            speed_profile,
            temp_channel_info,
            &hwmon_driver.name,
        )
        .await
        .map_err(|err| {
            anyhow!(
                "Error on {}:{channel_name} for speed profile {speed_profile:?} - {err}",
                hwmon_driver.name
            )
        });
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
            "Applying Lighting settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying LCD settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying pwm_mode setting is no longer supported for HWMON devices"
        ))
        // let (hwmon_driver, channel_info) = self.get_hwmon_info(device_uid, channel_name)?;
        // info!(
        //     "Applying HWMON device: {} channel: {}; PWM Mode: {}",
        //     device_uid, channel_name, pwm_mode
        // );
        // fans::set_pwm_mode(&hwmon_driver.path, channel_info, Some(pwm_mode)).await
    }

    async fn prepare_for_sleep(&self) {
        // Suspend prep runs in a tight systemd-sleep window (the
        // sleep notification fires 1-3 s before actual suspend). No
        // device permit is taken here: ThinkPad EC tolerates
        // concurrent ops with the preload loop, and waiting on the
        // permit could blow the suspend budget. The only protection
        // needed is a short write timeout so a wedged EC cannot
        // block suspend. All failures are logged and swallowed.
        for (device_uid, (_device_lock, hwmon_driver)) in &self.devices {
            if hwmon_driver.name != devices::DEVICE_NAME_THINK_PAD {
                continue;
            }
            for channel_info in &hwmon_driver.channels {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                if channel_info.caps.is_fan_controllable().not() {
                    continue;
                }
                info!(
                    "Setting ThinkPad device: {device_uid} channel: {} to auto mode for sleep",
                    channel_info.name
                );
                let write_fut = fans::set_pwm_enable(
                    fans::PWM_ENABLE_AUTO_VALUE,
                    &hwmon_driver.path,
                    channel_info,
                );
                match timeout(PREPARE_FOR_SLEEP_WRITE_TIMEOUT, write_fut).await {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        warn!(
                            "Failed to set auto mode for ThinkPad device: {device_uid} \
                             channel: {} before sleep: {err}",
                            channel_info.name
                        );
                    }
                    Err(_elapsed) => {
                        warn!(
                            "Timed out ({PREPARE_FOR_SLEEP_WRITE_TIMEOUT:?}) setting auto \
                             mode for ThinkPad device: {device_uid} channel: {} before \
                             sleep - EC may be wedged",
                            channel_info.name
                        );
                    }
                }
            }
        }
    }

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}

#[cfg(test)]
mod preload_tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::failsafe::{self, MISSING_DUTY_FAILSAFE, MISSING_RPM_FAILSAFE};
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    const TEST_TYPE_INDEX: TypeIndex = 1;

    struct PreloadContext {
        test_base_path: PathBuf,
    }

    async fn setup() -> PreloadContext {
        let base = format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4());
        let path = Path::new(&base).to_path_buf();
        cc_fs::create_dir_all(&path).await.unwrap();
        PreloadContext {
            test_base_path: path,
        }
    }

    async fn teardown(ctx: &PreloadContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).await.unwrap();
    }

    fn new_test_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        let mut repo = HwmonRepo::new(config, vec![]);
        // preload_device_statuses now acquires the device permit per
        // channel; tests need an entry for TEST_TYPE_INDEX or the
        // expect inside read_one_channel fires.
        repo.device_permits
            .insert(TEST_TYPE_INDEX, Rc::new(Semaphore::new(1)));
        repo
    }

    fn fan_channel_with_paths(number: u8, name: &str, base_path: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            pwm_enable_default: None,
            name: name.to_string(),
            label: None,
            caps: HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM,
            auto_curve: AutoCurveInfo::None,
            pwm_path: Some(base_path.join(format!("pwm{number}"))),
            rpm_path: Some(base_path.join(format!("fan{number}_input"))),
            temp_path: None,
        }
    }

    fn driver_with_channels(
        base_path: &Path,
        channels: Vec<HwmonChannelInfo>,
    ) -> Rc<HwmonDriverInfo> {
        Rc::new(HwmonDriverInfo {
            name: "test_driver".to_string(),
            path: base_path.to_path_buf(),
            model: None,
            u_id: String::new(),
            channels,
            block_dev_path: None,
            apple_smc: AppleMacSMC::default(),
        })
    }

    /// Seeds the failsafe map for `type_index` using initial statuses
    /// as if the device had successfully preloaded at init time.
    fn seed_failsafe(
        repo: &HwmonRepo,
        type_index: TypeIndex,
        channel_statuses: &[ChannelStatus],
        temp_statuses: &[TempStatus],
    ) {
        let (channel_failsafes, temp_failsafes) =
            failsafe::create_failsafe_data(channel_statuses, temp_statuses);
        if let Some(fsd) = FailsafeStatusData::new(channel_failsafes, temp_failsafes) {
            repo.failsafe_statuses.borrow_mut().insert(type_index, fsd);
        }
    }

    #[test]
    #[serial]
    fn preload_upserts_fresh_channel_in_place() {
        // Two successive preloads with fresh fan readings must replace
        // the cache entry in place rather than duplicating it.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // when: two consecutive preloads.
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            cc_fs::write(base.join("fan1_input"), b"1800".to_vec())
                .await
                .unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: cache has exactly one entry, with the latest value.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                assert_eq!(channels[0].name, "fan1");
                assert_eq!(channels[0].rpm, Some(1800));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_preserves_cache_on_single_channel_failure() {
        // When one of two fan channels fails its PWM read while the
        // other succeeds, the successful entry updates and the failing
        // entry keeps its prior last-known-good value.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            // both readable in the initial tick
            cc_fs::write(base.join("pwm1"), b"64".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"900".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("pwm2"), b"200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan2_input"), b"2500".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(
                base,
                vec![
                    fan_channel_with_paths(1, "fan1", base),
                    fan_channel_with_paths(2, "fan2", base),
                ],
            );
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // given: first preload populates both entries
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            // when: fan1 updates, fan2 now fails (pwm2 removed)
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            cc_fs::remove_file(base.join("pwm2")).await.unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: fan1 updated, fan2 preserved at 2500.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 2);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(1200));
                let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
                assert_eq!(fan2.rpm, Some(2500));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_applies_failsafe_only_when_threshold_exceeded() {
        // Drives the failsafe counter past MISSING_STATUS_THRESHOLD via
        // repeated failing preloads. Once active, the overlay replaces
        // the absent channel's cache entry with its failsafe value.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();

            // given: initial successful read to seed cache + failsafe data.
            let seed_status = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_status], &[]);
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // when: remove pwm1 so every subsequent preload fails, and
            // drive the counter above MISSING_STATUS_THRESHOLD.
            cc_fs::remove_file(base.join("pwm1")).await.unwrap();
            for _ in 0..=MISSING_STATUS_THRESHOLD {
                repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            }

            // then: the cache now holds the failsafe values for fan1,
            // because the overlay substituted them while the threshold
            // was exceeded and the channel did not report.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(MISSING_RPM_FAILSAFE));
                assert_eq!(fan1.duty, Some(MISSING_DUTY_FAILSAFE));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_recovery_clears_failsafe_on_success() {
        // After the per-channel stale counter trips the threshold and
        // fan1's cache entry is substituted with its failsafe value,
        // a fully successful preload must reset the counter to 0 and
        // the fresh read's values must replace the failsafe values.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            let seed_status = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_status], &[]);
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            cc_fs::remove_file(base.join("pwm1")).await.unwrap();
            for _ in 0..=MISSING_STATUS_THRESHOLD {
                repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            }
            // Verify failsafe is active for fan1 on the per-channel
            // path.
            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                assert!(fsd.was_failsafing);
                let fan1_state = &fsd.channel_state["fan1"];
                assert!((fan1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
                assert!(fan1_state.is_failsafed);
            }

            // when: pwm1 comes back and preload succeeds.
            cc_fs::write(base.join("pwm1"), b"200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"2000".to_vec())
                .await
                .unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: per-channel counter cleared, not failsafing, and
            // fresh values in the cache.
            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                assert!(fsd.was_failsafing.not());
                let fan1_state = &fsd.channel_state["fan1"];
                assert_eq!(fan1_state.stale_ticks, 0);
                assert!(fan1_state.is_failsafed.not());
            }
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(2000));
            }
            teardown(&ctx).await;
        });
    }

    // --- per-channel staleness wiring ---

    /// Seeds `fresh_this_tick` flags on the failsafe state for the
    /// given names, simulating what the streaming sinks would have
    /// upserted during the current preload attempt.
    fn mark_fresh(
        repo: &HwmonRepo,
        type_index: TypeIndex,
        channel_names: &[&str],
        temp_names: &[&str],
    ) {
        let mut fsd_map = repo.failsafe_statuses.borrow_mut();
        let fsd = fsd_map.get_mut(&type_index).unwrap();
        fsd.reset_fresh_this_tick();
        for name in channel_names {
            fsd.mark_channel_fresh(name);
        }
        for name in temp_names {
            fsd.mark_temp_fresh(name);
        }
    }

    #[test]
    #[serial]
    fn timeout_arm_respects_fresh_this_tick_flags() {
        // Simulates a still-running preload that has upserted fan1
        // but is stuck before fan2 / temp1. Repeated timeout-arm
        // firings must leave fan1's counter at 0 and its cache value
        // intact, while fan2 and temp1 tick up and fail over to
        // their failsafes once the threshold is crossed.
        let repo = new_test_repo();
        let seed_channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "fan2".to_string(),
                rpm: Some(900),
                duty: Some(30.0),
                ..Default::default()
            },
        ];
        let seed_temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 40.0,
        }];
        seed_failsafe(&repo, TEST_TYPE_INDEX, &seed_channels, &seed_temps);
        repo.preloaded_statuses
            .borrow_mut()
            .insert(TEST_TYPE_INDEX, (seed_channels, seed_temps));

        // In-flight preload state: only fan1 has been upserted.
        // Every tick re-applies the fresh flag (sink would fire once
        // per preload; since we only simulate the timeout-arm side,
        // the flag persists once set via `mark_channel_fresh`).
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            mark_fresh(&repo, TEST_TYPE_INDEX, &["fan1"], &[]);
            repo.tick_staleness_and_log(TEST_TYPE_INDEX, "test_driver");
        }

        let fsd_map = repo.failsafe_statuses.borrow();
        let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
        let fan1_state = &fsd.channel_state["fan1"];
        assert_eq!(fan1_state.stale_ticks, 0);
        assert!(fan1_state.is_failsafed.not());
        let fan2_state = &fsd.channel_state["fan2"];
        assert!((fan2_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(fan2_state.is_failsafed);
        let temp1_state = &fsd.temp_state["temp1"];
        assert!((temp1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(temp1_state.is_failsafed);
        assert!(fsd.was_failsafing);
        drop(fsd_map);

        let preloaded = repo.preloaded_statuses.borrow();
        let (channels, temps) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1200));
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, Some(MISSING_RPM_FAILSAFE));
        assert_eq!(fan2.duty, Some(MISSING_DUTY_FAILSAFE));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!((temp_entry.temp - failsafe::MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
    }

    #[test]
    #[serial]
    fn timeout_with_no_fresh_flags_ticks_everything() {
        // A preload that has not upserted anything (truly hung from
        // the start) leaves every `fresh_this_tick` flag false.
        // Every channel's counter must tick up and fail over once
        // the threshold is crossed.
        let repo = new_test_repo();
        let seed_channels = vec![ChannelStatus {
            name: "fan1".to_string(),
            rpm: Some(1200),
            duty: Some(50.0),
            ..Default::default()
        }];
        let seed_temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 40.0,
        }];
        seed_failsafe(&repo, TEST_TYPE_INDEX, &seed_channels, &seed_temps);
        repo.preloaded_statuses
            .borrow_mut()
            .insert(TEST_TYPE_INDEX, (seed_channels, seed_temps));
        // No fresh flags set - explicit reset to simulate a fresh
        // preload attempt that never upserts anything.
        mark_fresh(&repo, TEST_TYPE_INDEX, &[], &[]);

        for _ in 0..=MISSING_STATUS_THRESHOLD {
            repo.tick_staleness_and_log(TEST_TYPE_INDEX, "test_driver");
        }

        let fsd_map = repo.failsafe_statuses.borrow();
        let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
        let fan1_state = &fsd.channel_state["fan1"];
        assert!((fan1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(fan1_state.is_failsafed);
        let temp1_state = &fsd.temp_state["temp1"];
        assert!((temp1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(temp1_state.is_failsafed);
        assert!(fsd.was_failsafing);
        drop(fsd_map);

        let preloaded = repo.preloaded_statuses.borrow();
        let (channels, temps) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(MISSING_RPM_FAILSAFE));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!((temp_entry.temp - failsafe::MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
    }

    #[test]
    #[serial]
    fn preload_start_clears_fresh_this_tick_flags() {
        // The clear-at-start invariant: each preload attempt starts
        // with `fresh_this_tick` flags cleared, so flags left over
        // from a prior attempt cannot pretend to be fresh.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            let seed_fan = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_fan], &[]);
            // Pre-populate the fresh flag for fan1 from a prior
            // (simulated) preload attempt.
            mark_fresh(&repo, TEST_TYPE_INDEX, &["fan1"], &[]);

            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                // fan1 is fresh because this preload's sink fired,
                // not because of the pre-populated flag. Verified by
                // the stale_ticks counter staying at 0 (no tick up).
                assert_eq!(fsd.channel_state["fan1"].stale_ticks, 0);
                assert!(fsd.channel_state["fan1"].fresh_this_tick);
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_releases_permit_between_channels() {
        // Goal: per-channel permit handoff. After preload_device_statuses
        // returns, the device permit must be free, and the path the
        // function takes through the channels acquires and releases
        // it once per channel rather than holding it across the
        // whole device. This is what lets a writer slip in between
        // channel reads on a slow device.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            for n in 1..=2 {
                cc_fs::write(base.join(format!("pwm{n}")), b"128".to_vec())
                    .await
                    .unwrap();
                cc_fs::write(base.join(format!("fan{n}_input")), b"1200".to_vec())
                    .await
                    .unwrap();
            }
            let driver = driver_with_channels(
                base,
                vec![
                    fan_channel_with_paths(1, "fan1", base),
                    fan_channel_with_paths(2, "fan2", base),
                ],
            );
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // Run a preload while concurrently observing the
            // permit. Between channel reads the permit must be
            // briefly acquirable from the outside.
            let sem = Rc::clone(repo.device_permits.get(&TEST_TYPE_INDEX).unwrap());
            let observed_free = Rc::new(Cell::new(false));
            let observed_free_clone = Rc::clone(&observed_free);
            let observer = tokio::task::spawn_local(async move {
                for _ in 0_u32..50 {
                    if sem.try_acquire().is_ok() {
                        observed_free_clone.set(true);
                        return;
                    }
                    tokio::task::yield_now().await;
                }
            });
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            observer.await.unwrap();

            // After preload returns the permit must be unconditionally
            // free; the per-channel acquires were the last to hold it.
            let sem = repo.device_permits.get(&TEST_TYPE_INDEX).unwrap();
            assert!(
                sem.try_acquire().is_ok(),
                "device permit must be free after preload_device_statuses returns"
            );
            // Permit was at least transiently free during the preload
            // itself, demonstrating per-channel handoff. The check is
            // intentionally weak (one observation is enough) to keep
            // it stable in CI.
            assert!(
                observed_free.get(),
                "external observer never saw the permit free during preload"
            );
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn abort_pending_cancels_shutdown_token() {
        // Verifies the new Repository::abort_pending hook fires the
        // hwmon shutdown_token. The main loop relies on this so
        // spawned preload tasks self-bail before its moro scope
        // drops at shutdown.
        cc_fs::test_runtime(async {
            let repo = new_test_repo();
            assert!(
                repo.shutdown_token.is_cancelled().not(),
                "shutdown_token starts uncancelled"
            );
            repo.abort_pending().await;
            assert!(
                repo.shutdown_token.is_cancelled(),
                "abort_pending must cancel shutdown_token"
            );
        });
    }

    #[test]
    #[serial]
    fn preload_device_statuses_bails_when_shutdown_cancelled() {
        // Verifies the entry guard: once the shutdown token has been
        // cancelled, preload_device_statuses returns without reading
        // any channel and without upserting into preloaded_statuses.
        // This is what shortens the moro scope's wait at shutdown.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // given: shutdown already in progress.
            repo.abort_pending().await;

            // when:
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: no upserts landed for this type_index.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                assert!(
                    preloaded
                        .get(&TEST_TYPE_INDEX)
                        .is_none_or(|(channels, temps)| channels.is_empty() && temps.is_empty()),
                    "no channel/temp upserts must occur after shutdown_token is cancelled"
                );
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_device_statuses_bails_after_seeded_cache_when_cancelled() {
        // Verifies that once the cache has been seeded by an earlier
        // tick, a subsequent preload after shutdown_token is
        // cancelled does NOT overwrite the seeded values, even if
        // the underlying sysfs files have changed. Confirms the
        // entry guard short-circuits the read loop entirely.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // given: one good preload seeds the cache.
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            // change the underlying value so a re-read would update
            // the cache entry; if the bail-out skips the read, we
            // expect the original 1200 to remain.
            cc_fs::write(base.join("fan1_input"), b"3000".to_vec())
                .await
                .unwrap();

            // when: shutdown begins, then a preload runs.
            repo.abort_pending().await;
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: the cached value reflects the first preload, not
            // the second's would-be reading.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                assert_eq!(channels[0].name, "fan1");
                assert_eq!(
                    channels[0].rpm,
                    Some(1200),
                    "post-shutdown preload must not overwrite the seeded cache"
                );
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn read_one_channel_returns_true_on_shutdown_cancel() {
        // Verifies the shutdown arm in read_one_channel's permit
        // select returns true (treated as "permit acquired" so the
        // caller does not flag it as a slow-device timeout) and does
        // not reach the underlying sysfs read.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            // Intentionally do NOT create pwm1 / fan1_input. If the
            // function reached the read path it would log warnings
            // and the absence of an upsert would be ambiguous.
            let channel = fan_channel_with_paths(1, "fan1", base);
            let driver = driver_with_channels(base, vec![channel.clone()]);
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // given: shutdown already in progress.
            repo.abort_pending().await;

            // when:
            let acquired = repo
                .read_one_channel(
                    TEST_TYPE_INDEX,
                    &driver,
                    &channel,
                    &HwmonChannelType::Fan,
                    false,
                )
                .await;

            // then: select chose the shutdown arm; no upsert occurred.
            assert!(
                acquired,
                "shutdown arm must return true so log_slow_device is not triggered"
            );
            {
                let preloaded = repo.preloaded_statuses.borrow();
                assert!(
                    preloaded
                        .get(&TEST_TYPE_INDEX)
                        .is_none_or(|(channels, temps)| channels.is_empty() && temps.is_empty()),
                    "no upsert must occur on the shutdown-cancel arm"
                );
            }
            teardown(&ctx).await;
        });
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

    #[test]
    fn drivetemp_ioctl_timeout_scales_with_poll_rate() {
        // The ioctl budget must scale proportionally with poll_rate
        // so a slow drivetemp check cannot consume more than its
        // share of the overall read permit at any valid poll rate.
        assert_eq!(drivetemp_ioctl_timeout_for(0.5), Duration::from_millis(200));
        assert_eq!(drivetemp_ioctl_timeout_for(1.0), Duration::from_millis(400));
        assert_eq!(drivetemp_ioctl_timeout_for(5.0), Duration::from_secs(2));
    }

    #[test]
    fn drivetemp_ioctl_timeout_always_strictly_less_than_read_permit() {
        // Invariant: on ioctl timeout the fallback temp read must
        // still have budget left before the outer read permit arm
        // fires. Ratios 0.4 vs 0.7 preserve 3/7 headroom at every
        // poll rate.
        for poll_rate in [0.5_f64, 1.0, 5.0] {
            let ioctl = drivetemp_ioctl_timeout_for(poll_rate);
            let read = device_read_permit_timeout_for(poll_rate);
            assert!(
                ioctl < read,
                "ioctl must be < read permit at poll_rate={poll_rate}"
            );
        }
    }
}

#[cfg(test)]
mod command_delay_handoff_tests {
    use super::*;
    use crate::cc_fs;
    use serial_test::serial;

    const TEST_TYPE_INDEX: TypeIndex = 1;

    fn new_test_repo_with_permit() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        let mut repo = HwmonRepo::new(config, vec![]);
        repo.device_permits
            .insert(TEST_TYPE_INDEX, Rc::new(Semaphore::new(1)));
        repo
    }

    #[test]
    #[serial]
    fn delay_holder_is_noop_for_zero_delay() {
        // With delay_millis == 0 the handoff must not spawn a
        // delay-holder task. The permit stays free even after a
        // yield long enough for any spawned task to run.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, 0);
            sleep(Duration::from_millis(20)).await;
            let sem = repo.device_permits.get(&TEST_TYPE_INDEX).unwrap();
            assert!(
                sem.try_acquire().is_ok(),
                "permit must be free when delay is 0"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_call_returns_immediately() {
        // The caller of spawn_command_delay_holder must not stall
        // on the delay. Verify by measuring wall clock around the
        // call with a long configured delay.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            let start = Instant::now();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, 500);
            let elapsed = start.elapsed();
            assert!(
                elapsed < Duration::from_millis(50),
                "caller stalled: elapsed={elapsed:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_gates_permit_for_delay_duration() {
        // Core invariant: after handoff, the permit is held by
        // the detached task for approximately delay_millis and
        // then released. Subsequent writes / preloads that
        // acquire the same permit wait for the holder, but not
        // beyond.
        cc_fs::test_runtime(async {
            const DELAY_MS: u16 = 100;
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, DELAY_MS);
            // Yield so the spawn_local task can reach acquire
            // before we probe the permit state.
            sleep(Duration::from_millis(10)).await;
            let sem = Rc::clone(repo.device_permits.get(&TEST_TYPE_INDEX).unwrap());
            assert!(
                sem.try_acquire().is_err(),
                "permit must be held by delay task"
            );
            sleep(Duration::from_millis(u64::from(DELAY_MS) + 50)).await;
            assert!(
                sem.try_acquire().is_ok(),
                "permit must be released once delay elapses"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_is_noop_for_unknown_type_index() {
        // When no Semaphore exists for the given type_index, the
        // handoff must not panic; it should return silently.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX + 1, 100);
            sleep(Duration::from_millis(20)).await;
            let sem = repo.device_permits.get(&TEST_TYPE_INDEX).unwrap();
            assert!(sem.try_acquire().is_ok());
        });
    }
}

#[cfg(test)]
mod coalescer_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::DeviceInfo;
    use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
    use crate::repositories::hwmon::fans::duty_to_pwm_value;
    use serial_test::serial;
    use uuid::Uuid;

    fn fan_channel(number: u8, name: &str, base: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            name: name.to_string(),
            caps: HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::PWM
                | HwmonChannelCapabilities::RPM,
            pwm_path: Some(base.join(format!("pwm{number}"))),
            rpm_path: Some(base.join(format!("fan{number}_input"))),
            ..Default::default()
        }
    }

    async fn seed_fan_files(base: &Path, fan_numbers: &[u8]) {
        cc_fs::create_dir_all(base).await.unwrap();
        for &n in fan_numbers {
            cc_fs::write(base.join(format!("pwm{n}")), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join(format!("fan{n}_input")), b"1200".to_vec())
                .await
                .unwrap();
        }
    }

    fn empty_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    /// Registers a fake device with permit + writer mailbox in the
    /// repo, then spawns the writer task. Returns the device UID so
    /// tests can call `apply_setting_speed_fixed` on it.
    fn install_device_and_spawn_writer(
        repo: &mut HwmonRepo,
        type_index: TypeIndex,
        name: &str,
        path: PathBuf,
        channels: Vec<HwmonChannelInfo>,
        delay_millis: u16,
    ) -> UID {
        let driver = HwmonDriverInfo {
            name: name.to_string(),
            path,
            channels,
            u_id: format!("test-uid-{name}-{type_index}"),
            apple_smc: AppleMacSMC::default(),
            ..Default::default()
        };
        let device = Device::new(
            driver.name.clone(),
            DeviceType::Hwmon,
            type_index,
            None,
            DeviceInfo::default(),
            Some(driver.u_id.clone()),
            1.0,
        );
        let uid = device.uid.clone();
        repo.device_permits
            .insert(type_index, Rc::new(Semaphore::new(1)));
        repo.writers.insert(
            type_index,
            Rc::new(WriterMailbox {
                pending: RefCell::new(HashMap::with_capacity(PENDING_INITIAL_CAPACITY)),
                notify: Notify::new(),
            }),
        );
        repo.delay_logged.insert(type_index, Cell::new(0));
        if delay_millis > 0 {
            repo.device_delays.insert(uid.clone(), delay_millis);
        }
        let driver_rc = Rc::new(driver);
        repo.devices.insert(
            uid.clone(),
            (Rc::new(RefCell::new(device)), Rc::clone(&driver_rc)),
        );
        let task = WriterTask {
            type_index,
            mailbox: Rc::clone(repo.writers.get(&type_index).unwrap()),
            semaphore: Rc::clone(repo.device_permits.get(&type_index).unwrap()),
            driver: driver_rc,
            config: Rc::clone(&repo.config),
            preloaded_statuses: Rc::clone(&repo.preloaded_statuses),
            duty_cache: repo.duty_cache.get(&type_index).map(Rc::clone),
            shutdown: repo.shutdown_token.clone(),
            write_permit_timeout: repo.device_write_permit_timeout,
            delay_millis,
        };
        tokio::task::spawn_local(run_writer_task(task));
        uid
    }

    /// Spawns the `apply_setting_speed_fixed` future as a local task
    /// so it actually drives forward (enqueues into pending and
    /// awaits rx) before the test inspects state. Without this, an
    /// async fn returns a lazy future that does nothing until polled.
    fn enqueue_write(
        repo: &Rc<HwmonRepo>,
        uid: &UID,
        channel: &str,
        duty: u8,
    ) -> tokio::task::JoinHandle<Result<()>> {
        let repo = Rc::clone(repo);
        let uid = uid.clone();
        let channel = channel.to_string();
        tokio::task::spawn_local(async move {
            repo.apply_setting_speed_fixed(&uid, &channel, duty).await
        })
    }

    #[test]
    #[serial]
    fn coalescer_collapses_burst_to_single_write() {
        // Goal: a burst of writes to the same channel resolves with
        // the latest target as the final pwm value, and every
        // caller receives Ok. With the permit released, the
        // coalescer must produce a final state that matches the
        // last-written duty regardless of how the burst is sliced
        // between in-flight and pending.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );
            let repo = Rc::new(repo);
            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();

            // Burst five writes; each new value supersedes the prior.
            let mut handles = Vec::with_capacity(5);
            for duty in [10_u8, 20, 30, 40, 50] {
                handles.push(enqueue_write(&repo, &uid, "fan1", duty));
            }
            sleep(Duration::from_millis(20)).await;

            drop(permit_hold);
            for handle in handles {
                handle
                    .await
                    .expect("join should not fail")
                    .expect("each waiter must resolve Ok");
            }
            let pwm_after = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(
                pwm_after.trim(),
                duty_to_pwm_value(50).to_string(),
                "final pwm value must reflect the latest target (duty=50)"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_pending_merges_waiters_per_channel() {
        // Goal: once the writer is blocked on acquire (in-flight
        // first write), additional writes to the same channel
        // accumulate as a single pending entry with their waiters
        // merged. The mailbox bound stays at one entry per channel.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );
            let repo = Rc::new(repo);
            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();

            // First write: writer drains it into its local frame
            // and blocks on acquire while we hold the permit.
            let h0 = enqueue_write(&repo, &uid, "fan1", 10);
            sleep(Duration::from_millis(30)).await;

            // Subsequent writes accumulate in the mailbox's pending
            // map because the writer is still blocked.
            let h1 = enqueue_write(&repo, &uid, "fan1", 20);
            let h2 = enqueue_write(&repo, &uid, "fan1", 30);
            let h3 = enqueue_write(&repo, &uid, "fan1", 40);
            let h4 = enqueue_write(&repo, &uid, "fan1", 50);
            sleep(Duration::from_millis(20)).await;

            {
                let pending = repo.writers.get(&1).unwrap().pending.borrow();
                assert_eq!(pending.len(), 1, "one channel, one pending entry");
                let entry = pending.get("fan1").expect("entry must exist");
                assert_eq!(entry.target_duty, 50, "latest target wins");
                assert_eq!(entry.waiters.len(), 4, "four waiters merged");
            }

            drop(permit_hold);
            for handle in [h0, h1, h2, h3, h4] {
                handle.await.unwrap().unwrap();
            }
            let pwm_after = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(
                pwm_after.trim(),
                duty_to_pwm_value(50).to_string(),
                "final pwm value must reflect the latest target"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_responders_all_see_same_error() {
        // Goal: when the hardware write fails, every waiter merged
        // into the surviving entry receives the same error so no
        // caller is misled into thinking its write succeeded.
        cc_fs::test_runtime(async {
            // Skip seeding pwm1: set_pwm_duty's write fails because
            // the parent path doesn't exist.
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("missing-dev");
            cc_fs::create_dir_all(&base).await.unwrap();

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );
            let repo = Rc::new(repo);
            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();
            let h1 = enqueue_write(&repo, &uid, "fan1", 30);
            let h2 = enqueue_write(&repo, &uid, "fan1", 40);
            let h3 = enqueue_write(&repo, &uid, "fan1", 50);
            sleep(Duration::from_millis(20)).await;
            drop(permit_hold);

            let r1 = h1.await.unwrap();
            let r2 = h2.await.unwrap();
            let r3 = h3.await.unwrap();
            assert!(r1.is_err(), "write must fail when path is invalid");
            assert!(r2.is_err());
            assert!(r3.is_err());
            let m1 = r1.unwrap_err().to_string();
            let m2 = r2.unwrap_err().to_string();
            let m3 = r3.unwrap_err().to_string();
            assert_eq!(m1, m2);
            assert_eq!(m2, m3);

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_separate_channels_independent() {
        // Goal: writes to different channels do not coalesce. With
        // the permit released, both channel writes complete and
        // their pwm files reflect the requested values.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1, 2]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir), fan_channel(2, "fan2", &dir)],
                0,
            );

            let h1 = repo.apply_setting_speed_fixed(&uid, "fan1", 50);
            let h2 = repo.apply_setting_speed_fixed(&uid, "fan2", 75);
            h1.await.unwrap();
            h2.await.unwrap();

            let pwm1 = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            let pwm2 = cc_fs::read_sysfs(&dir.join("pwm2")).await.unwrap();
            assert_eq!(pwm1.trim(), duty_to_pwm_value(50).to_string());
            assert_eq!(pwm2.trim(), duty_to_pwm_value(75).to_string());

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_honors_command_delay() {
        // Goal: the writer task sleeps for delay_millis between
        // hardware writes. Sequential writes to two channels under a
        // 100 ms delay must take at least 100 ms total since the
        // second write only starts after the first's delay elapses.
        cc_fs::test_runtime(async {
            const DELAY_MS: u16 = 100;
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1, 2]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir), fan_channel(2, "fan2", &dir)],
                DELAY_MS,
            );
            let repo = Rc::new(repo);

            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();
            let h1 = enqueue_write(&repo, &uid, "fan1", 50);
            let h2 = enqueue_write(&repo, &uid, "fan2", 60);
            sleep(Duration::from_millis(20)).await;
            let start = Instant::now();
            drop(permit_hold);
            h1.await.unwrap().unwrap();
            h2.await.unwrap().unwrap();
            let elapsed = start.elapsed();
            assert!(
                elapsed >= Duration::from_millis(u64::from(DELAY_MS)),
                "two writes with {DELAY_MS}ms delay must serialize: elapsed={elapsed:?}"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_drain_during_inflight_creates_next_pending() {
        // Goal: a write arriving while the writer is mid-iteration
        // (during the command delay) lands in the freshly-empty
        // pending map and runs on the next loop iteration, never
        // silently merging into the in-flight write.
        cc_fs::test_runtime(async {
            const DELAY_MS: u16 = 100;
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                DELAY_MS,
            );
            let repo = Rc::new(repo);

            // First write: spawn so it actually progresses to rx.await
            // before we issue the second one.
            let h_first = enqueue_write(&repo, &uid, "fan1", 30);
            // Wait long enough for the writer to drain pending and
            // enter the per-write command delay.
            sleep(Duration::from_millis(30)).await;
            // Second write must land in a fresh pending entry
            // because the writer already drained the first one.
            let h_second = enqueue_write(&repo, &uid, "fan1", 70);

            h_first.await.unwrap().unwrap();
            h_second.await.unwrap().unwrap();
            let pwm = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(
                pwm.trim(),
                duty_to_pwm_value(70).to_string(),
                "second write must observably hit hardware after the first"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_per_device_isolation() {
        // Goal: a wedged writer on device A must not block writes
        // to device B, since each device owns its own permit, mailbox
        // and writer task.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir_a = base.join("dev_a");
            let dir_b = base.join("dev_b");
            seed_fan_files(&dir_a, &[1]).await;
            seed_fan_files(&dir_b, &[1]).await;

            let mut repo = empty_repo();
            let uid_a = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev_a",
                dir_a.clone(),
                vec![fan_channel(1, "fan1", &dir_a)],
                0,
            );
            let uid_b = install_device_and_spawn_writer(
                &mut repo,
                2,
                "dev_b",
                dir_b.clone(),
                vec![fan_channel(1, "fan1", &dir_b)],
                0,
            );
            let repo = Rc::new(repo);

            let permit_a_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_a = permit_a_sem.acquire().await.unwrap();
            let h_a = enqueue_write(&repo, &uid_a, "fan1", 50);
            let h_b = enqueue_write(&repo, &uid_b, "fan1", 75);

            let b_start = Instant::now();
            h_b.await.unwrap().unwrap();
            let b_elapsed = b_start.elapsed();
            assert!(
                b_elapsed < Duration::from_millis(500),
                "device B should not be blocked by device A: elapsed={b_elapsed:?}"
            );
            let pwm_b = cc_fs::read_sysfs(&dir_b.join("pwm1")).await.unwrap();
            assert_eq!(pwm_b.trim(), duty_to_pwm_value(75).to_string());

            drop(permit_a);
            h_a.await.unwrap().unwrap();

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_waiters_overflow_drops_oldest() {
        // Goal: if more than MAX_WAITERS_PER_PENDING_WRITE waiters
        // pile up on one channel, the oldest senders are dropped
        // with an overflow error so the merged list stays bounded.
        // Newer waiters survive and observe the eventual result.
        cc_fs::test_runtime(async {
            const EXTRAS: usize = 5;
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );
            let repo = Rc::new(repo);
            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();

            // First write so the writer drains it and blocks on
            // acquire. Subsequent writes then accumulate into the
            // mailbox's pending entry without being drained.
            let h_first = enqueue_write(&repo, &uid, "fan1", 1);
            sleep(Duration::from_millis(30)).await;

            // Push enough additional writes to overflow the cap.
            let total = MAX_WAITERS_PER_PENDING_WRITE + EXTRAS;
            let mut handles = Vec::with_capacity(total);
            for i in 0..total {
                let duty = u8::try_from((i % 99) + 1).unwrap();
                handles.push(enqueue_write(&repo, &uid, "fan1", duty));
            }
            sleep(Duration::from_millis(50)).await;
            assert_eq!(
                repo.writers
                    .get(&1)
                    .unwrap()
                    .pending
                    .borrow()
                    .get("fan1")
                    .expect("pending entry should hold the merged waiters")
                    .waiters
                    .len(),
                MAX_WAITERS_PER_PENDING_WRITE,
                "merged waiters must not exceed the bound"
            );
            drop(permit_hold);

            // h_first is in-flight and unaffected by overflow.
            h_first.await.unwrap().unwrap();
            let mut overflow_count = 0_usize;
            let mut ok_count = 0_usize;
            for handle in handles {
                match handle.await.unwrap() {
                    Ok(()) => ok_count += 1,
                    Err(err) if err.to_string().contains("superseded") => overflow_count += 1,
                    Err(err) => panic!("unexpected error: {err}"),
                }
            }
            assert_eq!(
                overflow_count, EXTRAS,
                "exactly EXTRAS oldest must overflow"
            );
            assert_eq!(ok_count, MAX_WAITERS_PER_PENDING_WRITE);

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn coalescer_cancellation_drops_pending_writes() {
        // Goal: when the repo's shutdown token fires, the writer
        // task drains pending entries and signals every waiter with
        // a cancelled error so callers do not hang past daemon exit.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );
            let repo = Rc::new(repo);
            let permit_sem = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_hold = permit_sem.acquire().await.unwrap();

            let h1 = enqueue_write(&repo, &uid, "fan1", 30);
            let h2 = enqueue_write(&repo, &uid, "fan1", 60);
            let h3 = enqueue_write(&repo, &uid, "fan1", 90);
            // Wait for the spawned tasks to reach rx.await and land
            // their entry in pending before we cancel.
            sleep(Duration::from_millis(20)).await;

            repo.shutdown_token.cancel();
            // Permit stays held; writer must still resolve waiters
            // because its cancel-arm fires before any acquire.
            let r1 = h1.await.unwrap().unwrap_err().to_string();
            let r2 = h2.await.unwrap().unwrap_err().to_string();
            let r3 = h3.await.unwrap().unwrap_err().to_string();
            assert!(r1.contains("cancelled"), "{r1}");
            assert!(r2.contains("cancelled"), "{r2}");
            assert!(r3.contains("cancelled"), "{r3}");

            drop(permit_hold);
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn fast_device_no_added_latency() {
        // Goal: with no contention the writer-task path stays fast
        // enough that the existing tick budget is not regressed.
        // 200 sequential calls must average well under 5 ms each.
        const ITERATIONS: u32 = 200;
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1]).await;

            let mut repo = empty_repo();
            let uid = install_device_and_spawn_writer(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                0,
            );

            let start = Instant::now();
            for i in 0..ITERATIONS {
                let duty = u8::try_from(i % 100).unwrap();
                repo.apply_setting_speed_fixed(&uid, "fan1", duty)
                    .await
                    .unwrap();
            }
            let elapsed = start.elapsed();
            let avg = elapsed / ITERATIONS;
            // Generous bound: the writer roundtrip on a healthy
            // host is well under a millisecond. 5 ms keeps CI
            // flakiness low without hiding a real regression.
            assert!(
                avg < Duration::from_millis(5),
                "average end-to-end {avg:?} regressed past 5 ms over {ITERATIONS} iterations"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }
}

#[cfg(test)]
mod slow_device_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::DeviceInfo;
    use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
    use crate::repositories::hwmon::fans::duty_to_pwm_value;
    use serial_test::serial;
    use uuid::Uuid;

    const TEST_TYPE_INDEX: TypeIndex = 1;

    fn fan_channel(number: u8, name: &str, base: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            name: name.to_string(),
            caps: HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::PWM
                | HwmonChannelCapabilities::RPM,
            pwm_path: Some(base.join(format!("pwm{number}"))),
            rpm_path: Some(base.join(format!("fan{number}_input"))),
            ..Default::default()
        }
    }

    async fn seed_fan_files(base: &Path, fan_numbers: &[u8], pwm_value: u8) {
        cc_fs::create_dir_all(base).await.unwrap();
        for &n in fan_numbers {
            cc_fs::write(
                base.join(format!("pwm{n}")),
                pwm_value.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(base.join(format!("fan{n}_input")), b"1200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join(format!("pwm{n}_enable")), b"1".to_vec())
                .await
                .unwrap();
        }
    }

    fn empty_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    /// Inserts a fake device + permit + writer mailbox + slow-flag
    /// + duty cache. Spawns the writer task. Returns the device UID.
    /// `slow == true` populates `slow_devices` and seeds `duty_cache`
    /// with the supplied cached entries; `slow == false` skips both.
    fn install_device(
        repo: &mut HwmonRepo,
        type_index: TypeIndex,
        name: &str,
        path: PathBuf,
        channels: Vec<HwmonChannelInfo>,
        slow: bool,
        cached_entries: Vec<(&str, Duty, Instant)>,
    ) -> UID {
        let driver = HwmonDriverInfo {
            name: name.to_string(),
            path,
            channels,
            u_id: format!("test-uid-{name}-{type_index}"),
            apple_smc: AppleMacSMC::default(),
            ..Default::default()
        };
        let device = Device::new(
            driver.name.clone(),
            DeviceType::Hwmon,
            type_index,
            None,
            DeviceInfo::default(),
            Some(driver.u_id.clone()),
            1.0,
        );
        let uid = device.uid.clone();
        repo.device_permits
            .insert(type_index, Rc::new(Semaphore::new(1)));
        repo.writers.insert(
            type_index,
            Rc::new(WriterMailbox {
                pending: RefCell::new(HashMap::with_capacity(PENDING_INITIAL_CAPACITY)),
                notify: Notify::new(),
            }),
        );
        repo.delay_logged.insert(type_index, Cell::new(0));
        if slow {
            repo.slow_devices.insert(type_index);
            let mut entries = HashMap::new();
            for (name, duty, verify_at) in cached_entries {
                entries.insert(
                    name.to_string(),
                    DutyCacheEntry {
                        last_known: duty,
                        next_verify_at: verify_at,
                    },
                );
            }
            repo.duty_cache
                .insert(type_index, Rc::new(RefCell::new(entries)));
        }
        let driver_rc = Rc::new(driver);
        repo.devices.insert(
            uid.clone(),
            (Rc::new(RefCell::new(device)), Rc::clone(&driver_rc)),
        );
        let task = WriterTask {
            type_index,
            mailbox: Rc::clone(repo.writers.get(&type_index).unwrap()),
            semaphore: Rc::clone(repo.device_permits.get(&type_index).unwrap()),
            driver: driver_rc,
            config: Rc::clone(&repo.config),
            preloaded_statuses: Rc::clone(&repo.preloaded_statuses),
            duty_cache: repo.duty_cache.get(&type_index).map(Rc::clone),
            shutdown: repo.shutdown_token.clone(),
            write_permit_timeout: repo.device_write_permit_timeout,
            delay_millis: 0,
        };
        tokio::task::spawn_local(run_writer_task(task));
        uid
    }

    fn enqueue_write(
        repo: &Rc<HwmonRepo>,
        uid: &UID,
        channel: &str,
        duty: u8,
    ) -> tokio::task::JoinHandle<Result<()>> {
        let repo = Rc::clone(repo);
        let uid = uid.clone();
        let channel = channel.to_string();
        tokio::task::spawn_local(async move {
            repo.apply_setting_speed_fixed(&uid, &channel, duty).await
        })
    }

    fn seed_failsafe(repo: &HwmonRepo, type_index: TypeIndex, channel_statuses: &[ChannelStatus]) {
        let (cf, tf) = failsafe::create_failsafe_data(channel_statuses, &[]);
        if let Some(fsd) = FailsafeStatusData::new(cf, tf) {
            repo.failsafe_statuses.borrow_mut().insert(type_index, fsd);
        }
    }

    #[test]
    #[serial]
    fn round_robin_rotates_fan_start_index_per_tick() {
        // Goal: successive preloads with N=3 fan channels insert
        // them into preloaded_statuses in rotated order. The Vec
        // is cleared between ticks because upsert_single_channel
        // updates in place; we want to observe the iteration
        // order, not the long-lived order.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1, 2, 3], 128).await;

            let mut repo = empty_repo();
            let _uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![
                    fan_channel(1, "fan1", &dir),
                    fan_channel(2, "fan2", &dir),
                    fan_channel(3, "fan3", &dir),
                ],
                false,
                vec![],
            );
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[]);
            let driver = Rc::clone(&repo.devices.values().next().unwrap().1);

            let mut orders: Vec<Vec<String>> = Vec::with_capacity(3);
            for tick in 0..3_u64 {
                repo.preloaded_statuses
                    .borrow_mut()
                    .remove(&TEST_TYPE_INDEX);
                repo.tick_count.set(tick);
                repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
                let order: Vec<String> = repo
                    .preloaded_statuses
                    .borrow()
                    .get(&TEST_TYPE_INDEX)
                    .unwrap()
                    .0
                    .iter()
                    .map(|c| c.name.clone())
                    .collect();
                orders.push(order);
            }
            assert_eq!(orders[0], vec!["fan1", "fan2", "fan3"]);
            assert_eq!(orders[1], vec!["fan2", "fan3", "fan1"]);
            assert_eq!(orders[2], vec!["fan3", "fan1", "fan2"]);

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn slow_device_preload_uses_cached_duty_until_verify_due() {
        // Goal: on a slow device, preload uses the cached duty
        // (no PWM read) while next_verify_at is in the future,
        // and triggers a real PWM read once it elapses.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            // pwm1 file says duty=255 (=> 100%); cache says 50.
            // If preload uses the cache, we see 50 in the upsert.
            // If it does a real read, we see 100.
            seed_fan_files(&dir, &[1], 255).await;
            let mut repo = empty_repo();
            let future_verify = Instant::now() + Duration::from_secs(60);
            let _uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                true,
                vec![("fan1", 50, future_verify)],
            );
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[]);
            let driver = Rc::clone(&repo.devices.values().next().unwrap().1);

            // Cache is fresh: cached duty 50 is used.
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            let cached_seen = repo
                .preloaded_statuses
                .borrow()
                .get(&TEST_TYPE_INDEX)
                .unwrap()
                .0[0]
                .duty;
            assert_eq!(cached_seen, Some(50.0), "should use cached duty");

            // Force verify by moving next_verify_at into the past;
            // next preload reads the real value (100 from pwm=255).
            {
                let mut entries = repo.duty_cache[&TEST_TYPE_INDEX].borrow_mut();
                let entry = entries.get_mut("fan1").unwrap();
                entry.next_verify_at = Instant::now() - Duration::from_secs(1);
            }
            repo.preloaded_statuses
                .borrow_mut()
                .remove(&TEST_TYPE_INDEX);
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            let real_seen = repo
                .preloaded_statuses
                .borrow()
                .get(&TEST_TYPE_INDEX)
                .unwrap()
                .0[0]
                .duty;
            assert_eq!(
                real_seen,
                Some(100.0),
                "should refresh from sysfs once verify is due"
            );
            // Cache also refreshed. Borrow scoped so it drops
            // before the next await (clippy::await_holding_refcell_ref).
            {
                let entries = repo.duty_cache[&TEST_TYPE_INDEX].borrow();
                assert_eq!(entries["fan1"].last_known, 100);
            }

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn manual_control_invalidates_cache() {
        // Goal: apply_setting_manual_control removes the cache
        // entry so the next preload re-reads the real value
        // rather than serving a stale cached duty.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 128).await;
            let mut repo = empty_repo();
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                true,
                vec![("fan1", 50, Instant::now() + Duration::from_secs(60))],
            );
            assert_eq!(repo.duty_cache[&TEST_TYPE_INDEX].borrow().len(), 1);

            repo.apply_setting_manual_control(&uid, "fan1")
                .await
                .unwrap();

            assert!(
                repo.duty_cache[&TEST_TYPE_INDEX]
                    .borrow()
                    .get("fan1")
                    .is_none(),
                "manual_control must invalidate the cache entry"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn reset_invalidates_cache() {
        // Goal: apply_setting_reset removes the cache entry —
        // device returns to auto mode, so the cached "what we
        // last set" no longer reflects the live duty.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 128).await;
            // Channel needs pwm_enable_default for set_pwm_enable_to_default_or_auto.
            let mut channel = fan_channel(1, "fan1", &dir);
            channel.pwm_enable_default = Some(2);
            let mut repo = empty_repo();
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![channel],
                true,
                vec![("fan1", 50, Instant::now() + Duration::from_secs(60))],
            );
            assert_eq!(repo.duty_cache[&TEST_TYPE_INDEX].borrow().len(), 1);

            repo.apply_setting_reset(&uid, "fan1").await.unwrap();

            assert!(
                repo.duty_cache[&TEST_TYPE_INDEX]
                    .borrow()
                    .get("fan1")
                    .is_none(),
                "reset must invalidate the cache entry"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn write_skips_when_preloaded_status_matches_target() {
        // Goal: when preloaded_statuses already says duty=50 and
        // the writer task is asked to write 50, the sysfs file
        // is NOT touched. Observable: write the pwm file with a
        // sentinel before the test runs; if the writer skips,
        // the sentinel survives.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 99).await;
            // Place a sentinel in pwm1 we'll detect afterwards.
            cc_fs::write(dir.join("pwm1"), b"42".to_vec())
                .await
                .unwrap();

            let mut repo = empty_repo();
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                false,
                vec![],
            );
            // Seed preloaded_statuses with duty 50 for fan1.
            repo.preloaded_statuses.borrow_mut().insert(
                TEST_TYPE_INDEX,
                (
                    vec![ChannelStatus {
                        name: "fan1".to_string(),
                        duty: Some(50.0),
                        rpm: Some(1200),
                        ..Default::default()
                    }],
                    vec![],
                ),
            );
            let repo = Rc::new(repo);

            // Target equals current cached duty: should skip.
            enqueue_write(&repo, &uid, "fan1", 50)
                .await
                .unwrap()
                .unwrap();

            let pwm = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(
                pwm.trim(),
                "42",
                "sentinel must remain: write should have been skipped"
            );

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn write_proceeds_when_preloaded_status_differs() {
        // Goal: when preloaded_statuses says 50 and target is 60,
        // the writer issues a real sysfs write (so pwm1 reflects
        // duty_to_pwm_value(60), not the sentinel).
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 99).await;
            cc_fs::write(dir.join("pwm1"), b"42".to_vec())
                .await
                .unwrap();

            let mut repo = empty_repo();
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                false,
                vec![],
            );
            repo.preloaded_statuses.borrow_mut().insert(
                TEST_TYPE_INDEX,
                (
                    vec![ChannelStatus {
                        name: "fan1".to_string(),
                        duty: Some(50.0),
                        rpm: Some(1200),
                        ..Default::default()
                    }],
                    vec![],
                ),
            );
            let repo = Rc::new(repo);

            enqueue_write(&repo, &uid, "fan1", 60)
                .await
                .unwrap()
                .unwrap();

            let pwm = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(pwm.trim(), duty_to_pwm_value(60).to_string());

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn write_proceeds_when_preloaded_status_missing() {
        // Goal: empty preloaded_statuses is treated as "current
        // duty unknown"; the writer falls through to the real
        // sysfs write rather than spuriously skipping.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 99).await;

            let mut repo = empty_repo();
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                false,
                vec![],
            );
            // Intentionally do NOT seed preloaded_statuses.
            let repo = Rc::new(repo);

            enqueue_write(&repo, &uid, "fan1", 70)
                .await
                .unwrap()
                .unwrap();

            let pwm = cc_fs::read_sysfs(&dir.join("pwm1")).await.unwrap();
            assert_eq!(pwm.trim(), duty_to_pwm_value(70).to_string());

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn slow_device_duty_cache_updated_on_write() {
        // Goal: a successful write on a slow device updates the
        // cache's last_known so the next preload tick can use it
        // without hitting the slow PWM read again.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_fan_files(&dir, &[1], 99).await;

            let mut repo = empty_repo();
            let future_verify = Instant::now() + Duration::from_secs(60);
            let uid = install_device(
                &mut repo,
                TEST_TYPE_INDEX,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir)],
                true,
                vec![("fan1", 30, future_verify)],
            );
            // preloaded_statuses says 30 (matches cache); ensure
            // target differs so the write goes through.
            repo.preloaded_statuses.borrow_mut().insert(
                TEST_TYPE_INDEX,
                (
                    vec![ChannelStatus {
                        name: "fan1".to_string(),
                        duty: Some(30.0),
                        rpm: Some(1200),
                        ..Default::default()
                    }],
                    vec![],
                ),
            );
            let repo = Rc::new(repo);

            enqueue_write(&repo, &uid, "fan1", 80)
                .await
                .unwrap()
                .unwrap();

            // Borrow scoped so it drops before the next await
            // (clippy::await_holding_refcell_ref).
            {
                let entries = repo.duty_cache[&TEST_TYPE_INDEX].borrow();
                assert_eq!(
                    entries["fan1"].last_known, 80,
                    "slow-device cache must absorb the new write"
                );
                assert_eq!(
                    entries["fan1"].next_verify_at, future_verify,
                    "writes do not reset the verification clock"
                );
            }

            repo.shutdown_token.cancel();
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }
}

#[cfg(test)]
mod prepare_for_sleep_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::DeviceInfo;
    use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
    use serial_test::serial;
    use uuid::Uuid;

    async fn seed_pwm_dir(base: &Path, pwm_enable_initial: &[u8]) {
        cc_fs::create_dir_all(base).await.unwrap();
        cc_fs::write(base.join("pwm1_enable"), pwm_enable_initial.to_vec())
            .await
            .unwrap();
        cc_fs::write(base.join("pwm1"), b"128".to_vec())
            .await
            .unwrap();
        cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
            .await
            .unwrap();
    }

    fn thinkpad_fan(
        number: u8,
        name: &str,
        base: &Path,
        pwm_enable_default: Option<u8>,
    ) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            name: name.to_string(),
            pwm_enable_default,
            caps: HwmonChannelCapabilities::FAN_WRITABLE | HwmonChannelCapabilities::PWM,
            pwm_path: Some(base.join(format!("pwm{number}"))),
            rpm_path: Some(base.join(format!("fan{number}_input"))),
            ..Default::default()
        }
    }

    fn insert_thinkpad_device(
        repo: &mut HwmonRepo,
        type_index: TypeIndex,
        driver_path: PathBuf,
        channels: Vec<HwmonChannelInfo>,
    ) {
        let driver = HwmonDriverInfo {
            name: devices::DEVICE_NAME_THINK_PAD.to_string(),
            path: driver_path,
            channels,
            u_id: format!("test-uid-thinkpad-{type_index}"),
            apple_smc: AppleMacSMC::default(),
            ..Default::default()
        };
        let device = Device::new(
            driver.name.clone(),
            DeviceType::Hwmon,
            type_index,
            None,
            DeviceInfo::default(),
            Some(driver.u_id.clone()),
            1.0,
        );
        let uid = device.uid.clone();
        repo.device_permits
            .insert(type_index, Rc::new(Semaphore::new(1)));
        repo.delay_logged.insert(type_index, Cell::new(0));
        repo.devices
            .insert(uid, (Rc::new(RefCell::new(device)), Rc::new(driver)));
    }

    fn empty_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    #[test]
    #[serial]
    fn prepare_for_sleep_writes_auto_value() {
        // Happy path: a ThinkPad fan with a controllable permit is
        // switched to auto mode for suspend.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_pwm_dir(&dir, b"1").await;

            let mut repo = empty_repo();
            insert_thinkpad_device(
                &mut repo,
                1,
                dir.clone(),
                vec![thinkpad_fan(1, "fan1", &dir, Some(2))],
            );

            repo.prepare_for_sleep().await;

            let after = cc_fs::read_sysfs(&dir.join("pwm1_enable")).await.unwrap();
            assert_eq!(
                after.trim(),
                "2",
                "fan should be set to auto mode for sleep"
            );

            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn prepare_for_sleep_does_not_hang_on_wedged_ec_write() {
        // Verifies the write-timeout bound: if the pwm_enable write
        // hangs (simulated with a FIFO whose read side has no
        // reader, so open(2) for write blocks waiting to be paired),
        // prepare_for_sleep returns well within the suspend budget
        // rather than waiting on the kernel indefinitely.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            cc_fs::create_dir_all(&dir).await.unwrap();
            // pwm1_enable is a FIFO — a write-open call on a FIFO
            // with no reader blocks, so set_pwm_enable hangs inside
            // the tokio::fs layer.
            let fifo_path = dir.join("pwm1_enable");
            let path_c = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
            // SAFETY: CString is valid; mode is a standard POSIX
            // value; mkfifo is safe for these args.
            let rc = unsafe { nix::libc::mkfifo(path_c.as_ptr(), 0o600) };
            assert_eq!(
                rc,
                0,
                "mkfifo failed: errno={}",
                std::io::Error::last_os_error()
            );

            let mut repo = empty_repo();
            insert_thinkpad_device(
                &mut repo,
                1,
                dir.clone(),
                vec![thinkpad_fan(1, "fan1", &dir, Some(2))],
            );

            let start = Instant::now();
            repo.prepare_for_sleep().await;
            let elapsed = start.elapsed();

            // Pair the FIFO BEFORE the assertions: a panic here
            // would leave the leaked blocking write thread stuck in
            // open(), and the test runtime drop would hang forever
            // waiting for it.
            let fifo_for_reader = fifo_path.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = std::fs::OpenOptions::new()
                    .read(true)
                    .open(&fifo_for_reader);
            })
            .await;

            assert!(
                elapsed >= PREPARE_FOR_SLEEP_WRITE_TIMEOUT,
                "write timeout should have elapsed at least once: {elapsed:?}"
            );
            assert!(
                elapsed < PREPARE_FOR_SLEEP_WRITE_TIMEOUT + Duration::from_millis(500),
                "prepare_for_sleep ran past the write timeout: {elapsed:?}"
            );

            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::DeviceInfo;
    use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
    use serial_test::serial;
    use uuid::Uuid;

    fn fan_channel(
        number: u8,
        name: &str,
        base: &Path,
        pwm_enable_default: Option<u8>,
    ) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            name: name.to_string(),
            pwm_enable_default,
            caps: HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::PWM
                | HwmonChannelCapabilities::RPM,
            pwm_path: Some(base.join(format!("pwm{number}"))),
            rpm_path: Some(base.join(format!("fan{number}_input"))),
            ..Default::default()
        }
    }

    /// Seeds a subdirectory with the files needed for
    /// `set_pwm_enable_to_default_or_auto` to operate. Initial
    /// `pwm_enable_initial` should be "1" (manual) so the reset
    /// path actually writes.
    async fn seed_pwm_dir(base: &Path, pwm_enable_initial: &[u8]) {
        cc_fs::create_dir_all(base).await.unwrap();
        cc_fs::write(base.join("pwm1_enable"), pwm_enable_initial.to_vec())
            .await
            .unwrap();
        cc_fs::write(base.join("pwm1"), b"128".to_vec())
            .await
            .unwrap();
        cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
            .await
            .unwrap();
    }

    /// Manually registers a fake device in `repo` so the test is
    /// focused on the shutdown loop rather than the init machinery.
    /// `u_id` is built from `driver_name` + `type_index` so every device
    /// inserted has a unique `create_uid_from` hash (the function
    /// only uses `d_id` when Some, so sharing a default `""` `u_id`
    /// would collide every inserted device into a single `HashMap`
    /// entry).
    fn insert_device(
        repo: &mut HwmonRepo,
        type_index: TypeIndex,
        driver_name: &str,
        driver_path: PathBuf,
        channels: Vec<HwmonChannelInfo>,
    ) {
        let driver = HwmonDriverInfo {
            name: driver_name.to_string(),
            path: driver_path,
            channels,
            u_id: format!("test-uid-{driver_name}-{type_index}"),
            apple_smc: AppleMacSMC::default(),
            ..Default::default()
        };
        let device = Device::new(
            driver.name.clone(),
            DeviceType::Hwmon,
            type_index,
            None,
            DeviceInfo::default(),
            Some(driver.u_id.clone()),
            1.0,
        );
        let uid = device.uid.clone();
        repo.device_permits
            .insert(type_index, Rc::new(Semaphore::new(1)));
        repo.delay_logged.insert(type_index, Cell::new(0));
        repo.devices
            .insert(uid, (Rc::new(RefCell::new(device)), Rc::new(driver)));
    }

    fn empty_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    #[test]
    #[serial]
    fn shutdown_continues_after_permit_timeout_on_earlier_device() {
        // Verifies M2: when device A's permit is held by another
        // task, shutdown's acquire times out, logs the failure, and
        // proceeds to reset device B's channels rather than
        // bubbling out of the loop.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir_a = base.join("dev_a");
            let dir_b = base.join("dev_b");
            seed_pwm_dir(&dir_a, b"1").await;
            seed_pwm_dir(&dir_b, b"1").await;

            let mut repo = empty_repo();
            // Short write timeout so the test does not wait 8 s.
            repo.device_write_permit_timeout = Duration::from_millis(100);
            insert_device(
                &mut repo,
                1,
                "dev_a",
                dir_a.clone(),
                vec![fan_channel(1, "fan1", &dir_a, Some(2))],
            );
            insert_device(
                &mut repo,
                2,
                "dev_b",
                dir_b.clone(),
                vec![fan_channel(1, "fan1", &dir_b, Some(2))],
            );

            // Hold device A's permit so shutdown's acquire times out.
            // Keep the Rc clone alive as long as the permit to satisfy
            // the borrow checker; both are dropped at end of test.
            let sem_a = Rc::clone(repo.device_permits.get(&1).unwrap());
            let permit_a = sem_a.try_acquire().expect("permit A starts free");

            let result = repo.shutdown().await;

            assert!(result.is_err(), "shutdown should report failures");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("dev_a:fan1"),
                "error should mention failed channel: {err_msg}"
            );
            assert!(
                err_msg.contains("1 channel failure"),
                "error should report count: {err_msg}"
            );
            // dev_a is left at manual (not reset) because the permit
            // was held throughout its shutdown attempt.
            let a_after = cc_fs::read_sysfs(&dir_a.join("pwm1_enable")).await.unwrap();
            assert_eq!(a_after.trim(), "1", "dev_a should not have been reset");
            // dev_b is reset to the default (2) — proves the loop
            // continued past dev_a's failure.
            let b_after = cc_fs::read_sysfs(&dir_b.join("pwm1_enable")).await.unwrap();
            assert_eq!(b_after.trim(), "2", "dev_b should have been reset");

            drop(permit_a);
            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }

    #[test]
    #[serial]
    fn shutdown_returns_ok_on_happy_path() {
        // Regression: shutdown returns Ok and resets the channel
        // when no permit is contended and the writes succeed.
        cc_fs::test_runtime(async {
            let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let dir = base.join("dev");
            seed_pwm_dir(&dir, b"1").await;

            let mut repo = empty_repo();
            insert_device(
                &mut repo,
                1,
                "dev",
                dir.clone(),
                vec![fan_channel(1, "fan1", &dir, Some(2))],
            );

            let result = repo.shutdown().await;

            assert!(
                result.is_ok(),
                "happy-path shutdown should succeed: {result:?}"
            );
            let after = cc_fs::read_sysfs(&dir.join("pwm1_enable")).await.unwrap();
            assert_eq!(after.trim(), "2", "channel should be reset to default");

            let _ = cc_fs::remove_dir_all(&base).await;
        });
    }
}

#[cfg(test)]
mod synthesize_initial_statuses_tests {
    use super::*;

    #[test]
    fn fan_with_pwm_and_rpm_caps_seeds_both_fields() {
        // A fully-capable fan channel produces a stub with both rpm
        // and duty set — failsafe::create_failsafe_data preserves
        // both fields on the resulting failsafe value.
        let channels = vec![HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            name: "fan1".to_string(),
            caps: HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM,
            ..Default::default()
        }];
        let (chans, temps) = synthesize_initial_statuses(&channels);
        assert_eq!(chans.len(), 1);
        assert_eq!(temps.len(), 0);
        assert_eq!(chans[0].name, "fan1");
        assert_eq!(chans[0].rpm, Some(0));
        assert_eq!(chans[0].duty, Some(0.0));
    }

    #[test]
    fn fan_with_only_rpm_caps_omits_duty_field() {
        // Field presence on the stub matches caps so the failsafe
        // value won't claim a duty for a read-only RPM channel.
        let channels = vec![HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            name: "fan_rpm_only".to_string(),
            caps: HwmonChannelCapabilities::RPM,
            ..Default::default()
        }];
        let (chans, _) = synthesize_initial_statuses(&channels);
        assert_eq!(chans[0].rpm, Some(0));
        assert_eq!(chans[0].duty, None);
    }

    #[test]
    fn power_and_temp_channels_get_appropriate_stubs() {
        let channels = vec![
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Power,
                name: "power1".to_string(),
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Temp,
                name: "temp1".to_string(),
                ..Default::default()
            },
        ];
        let (chans, temps) = synthesize_initial_statuses(&channels);
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].name, "power1");
        assert_eq!(chans[0].watts, Some(0.0));
        assert_eq!(temps.len(), 1);
        assert_eq!(temps[0].name, "temp1");
    }

    #[test]
    fn unsupported_channel_types_are_skipped() {
        // Load / Freq / PowerCap are not preloaded by hwmon's
        // streaming extractors, so they have no failsafe entry.
        let channels = vec![
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Load,
                name: "load1".to_string(),
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Freq,
                name: "freq1".to_string(),
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::PowerCap,
                name: "powercap1".to_string(),
                ..Default::default()
            },
        ];
        let (chans, temps) = synthesize_initial_statuses(&channels);
        assert!(chans.is_empty());
        assert!(temps.is_empty());
    }
}

#[cfg(test)]
mod init_timeout_tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
    use serial_test::serial;
    use uuid::Uuid;

    async fn setup_dir() -> PathBuf {
        let base = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
        cc_fs::create_dir_all(&base).await.unwrap();
        base
    }

    async fn teardown_dir(base: &Path) {
        let _ = cc_fs::remove_dir_all(base).await;
    }

    fn temp_channel(number: u8, name: &str, temp_path: PathBuf) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number,
            name: name.to_string(),
            temp_path: Some(temp_path),
            ..Default::default()
        }
    }

    fn driver_for_test(
        name: &str,
        base: &Path,
        channels: Vec<HwmonChannelInfo>,
    ) -> HwmonDriverInfo {
        HwmonDriverInfo {
            name: name.to_string(),
            path: base.to_path_buf(),
            channels,
            apple_smc: AppleMacSMC::default(),
            ..Default::default()
        }
    }

    fn empty_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    #[test]
    #[serial]
    fn map_into_model_registers_device_on_happy_path() {
        // Regression: with readable sysfs files and a generous
        // timeout, the device is registered normally. Guards against
        // the timeout machinery breaking the happy path.
        cc_fs::test_runtime(async {
            let base = setup_dir().await;
            cc_fs::write(base.join("temp1_input"), b"40000".to_vec())
                .await
                .unwrap();
            let driver = driver_for_test(
                "test_ok",
                &base,
                vec![temp_channel(1, "temp1", base.join("temp1_input"))],
            );

            let mut repo = empty_repo();
            let result = repo
                .map_into_our_device_model(vec![driver], Duration::from_secs(5))
                .await;

            assert!(
                result.is_ok(),
                "map should succeed on happy path: {result:?}"
            );
            assert_eq!(repo.devices.len(), 1, "one device should be registered");

            teardown_dir(&base).await;
        });
    }

    #[test]
    #[serial]
    fn map_into_model_skips_device_on_hanging_temp_read() {
        // Verifies the core H2 invariant: a wedged sysfs file during
        // init cannot stall daemon startup. Uses a FIFO at the temp
        // channel's read path; the reader blocks in open(2) until a
        // writer connects. The extract_temp_statuses call therefore
        // hangs; the timeout fires; the device is skipped. After
        // validation the test pairs up the FIFO so the leaked
        // blocking task completes and the runtime drops cleanly.
        cc_fs::test_runtime(async {
            let base = setup_dir().await;
            let fifo_path = base.join("temp1_input");
            let path_c = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();
            // SAFETY: path is a valid CString; mode is a standard
            // POSIX mode; mkfifo is safe when called with these args.
            let rc = unsafe { nix::libc::mkfifo(path_c.as_ptr(), 0o600) };
            assert_eq!(
                rc,
                0,
                "mkfifo failed: errno={}",
                std::io::Error::last_os_error()
            );

            let driver = driver_for_test(
                "test_slow",
                &base,
                vec![temp_channel(1, "temp1", fifo_path.clone())],
            );

            let mut repo = empty_repo();
            let start = Instant::now();
            let result = repo
                .map_into_our_device_model(vec![driver], Duration::from_millis(200))
                .await;
            let elapsed = start.elapsed();

            assert!(result.is_ok(), "map must return Ok even on timeout");
            assert!(
                elapsed < Duration::from_millis(1500),
                "init timeout did not fire within budget: elapsed={elapsed:?}"
            );
            assert!(
                repo.devices.is_empty(),
                "device with hanging read should be skipped"
            );

            // Pair the FIFO so the blocking reader thread can
            // complete. Without this the runtime drop may wait on
            // the leaked read-open syscall.
            let fifo_for_writer = fifo_path.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&fifo_for_writer);
            })
            .await;

            teardown_dir(&base).await;
        });
    }

    fn fan_channel_no_files(name: &str, base: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            name: name.to_string(),
            pwm_enable_default: Some(2),
            caps: HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM,
            // Both paths intentionally point at non-existent files
            // so extract_fan_statuses fails and omits the channel
            // from its result Vec.
            pwm_path: Some(base.join("pwm1")),
            rpm_path: Some(base.join("fan1_input")),
            ..Default::default()
        }
    }

    #[test]
    #[serial]
    fn map_into_model_seeds_failsafe_for_channels_that_failed_to_read() {
        // Verifies L2: a fan channel whose first read fails at init
        // is still tracked by the per-channel failsafe state. Without
        // the synth-based seed, the failsafe map would only contain
        // channels that successfully read, and a channel whose
        // sensor was momentarily unreadable would never surface to
        // the UI.
        cc_fs::test_runtime(async {
            let base = setup_dir().await;
            let driver = driver_for_test(
                "test_no_files",
                &base,
                vec![fan_channel_no_files("fan1", &base)],
            );

            let mut repo = empty_repo();
            let result = repo
                .map_into_our_device_model(vec![driver], Duration::from_secs(2))
                .await;
            assert!(result.is_ok(), "map should succeed even if reads failed");
            assert_eq!(repo.devices.len(), 1, "device should be registered");

            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map
                    .get(&1)
                    .expect("failsafe data exists for the device");
                assert!(
                    fsd.channel_state.contains_key("fan1"),
                    "fan1 should be tracked in per-channel state despite init read failure"
                );
                assert!(
                    fsd.channel_failsafes.contains_key("fan1"),
                    "fan1 should have a failsafe value even if it never read successfully"
                );
            }

            teardown_dir(&base).await;
        });
    }
}
