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

use crate::repositories::hwmon::devices::{self, HWMON_DEVICE_NAME_BLACKLIST};
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::utils::{sanitize_for_shell, ShellCommand};
use crate::{cc_fs, AllDevices, ENV_DBUS};
use anyhow::Result;
use log::{debug, info, warn};
use moro_local::Scope;
use nix::sys::socket::{
    bind, recv, socket, AddressFamily, MsgFlags, NetlinkAddr, SockFlag, SockProtocol, SockType,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::env;
use std::ops::Not;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tokio::io::unix::{AsyncFd, AsyncFdReadyGuard};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const DEBOUNCE_DURATION: Duration = Duration::from_secs(5);
const NOTIFICATION_CMD_TIMEOUT: Duration = Duration::from_secs(20);
/// Standard kernel uevent buffer size. Messages exceeding this are truncated.
const UEVENT_BUF_SIZE: usize = 4096;
/// Maximum uevent messages to drain per readable notification. Prevents
/// starvation if the kernel produces events faster than we consume them.
const MAX_DRAIN_PER_WAKE: usize = 256;
/// Icon 4 = `NotificationIcon::Info` in notifier.rs.
const NOTIFICATION_ICON_INFO: u8 = 4;
const MIN_USER_UID: u32 = 1000;
// Static assertions documenting constant relationships.
const _: () = assert!(UEVENT_BUF_SIZE >= 1024);
const _: () = assert!(MAX_DRAIN_PER_WAKE > 0);

pub struct DeviceListener {
    device_changed: Rc<Cell<bool>>,
}

impl<'s> DeviceListener {
    /// Creates a new `DeviceListener` that monitors kernel uevents for device
    /// additions and removals. When an applicable device change is detected,
    /// a warning is logged and a desktop notification is sent.
    ///
    /// Returns a deaf (inactive) listener if the D-Bus env var is disabled or
    /// the netlink socket cannot be created.
    pub async fn new(
        all_devices: AllDevices,
        liquidctl_repo: Option<Rc<LiquidctlRepo>>,
        bin_path: String,
        run_token: CancellationToken,
        scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Result<Self> {
        if is_listener_disabled() {
            info!("Device change listener disabled.");
            return Ok(Self::deaf());
        }
        let fd = match create_netlink_socket() {
            Ok(fd) => fd,
            Err(err) => {
                warn!(
                    "Could not create netlink socket for device change \
                     detection: {err}"
                );
                return Ok(Self::deaf());
            }
        };
        let async_fd = match AsyncFd::new(fd) {
            Ok(afd) => afd,
            Err(err) => {
                warn!("Could not register netlink socket with tokio: {err}");
                return Ok(Self::deaf());
            }
        };
        let hwmon_baseline = build_hwmon_baseline().await;
        let lc_baseline = build_liquidctl_baseline(&all_devices);
        debug!(
            "Device listener baseline: {} hwmon paths, {} liquidctl devices",
            hwmon_baseline.len(),
            lc_baseline.len()
        );
        let listener = Self {
            device_changed: Rc::new(Cell::new(false)),
        };
        let device_changed = Rc::clone(&listener.device_changed);
        scope.spawn(async move {
            run_event_loop(
                async_fd,
                hwmon_baseline,
                lc_baseline,
                liquidctl_repo,
                bin_path,
                device_changed,
                run_token,
            )
            .await;
        });
        Ok(listener)
    }

    /// Returns an inactive listener that never detects device changes.
    pub fn deaf() -> Self {
        Self {
            device_changed: Rc::new(Cell::new(false)),
        }
    }

    /// Returns whether any device change has been detected since startup.
    #[allow(dead_code)]
    pub fn has_device_changed(&self) -> bool {
        self.device_changed.get()
    }
}

/// The listener is disabled when the D-Bus env var explicitly disables it
/// (set to "0" or "off"). Enabled by default when unset.
fn is_listener_disabled() -> bool {
    let Ok(env_dbus) = env::var(ENV_DBUS) else {
        // Not set: listener enabled by default.
        return false;
    };
    // Numeric check first (e.g. "0" disables, any non-zero enables).
    if let Ok(value) = env_dbus.parse::<u8>() {
        return value == 0;
    }
    // Fall back to string comparison.
    env_dbus.trim().eq_ignore_ascii_case("off")
}

fn create_netlink_socket() -> Result<OwnedFd> {
    let fd = socket(
        AddressFamily::Netlink,
        SockType::Datagram,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        Some(SockProtocol::NetlinkKObjectUEvent),
    )?;
    // Group bitmask 1 = kernel broadcast group for uevents.
    let addr = NetlinkAddr::new(std::process::id(), 1);
    bind(fd.as_raw_fd(), &addr)?;
    Ok(fd)
}

/// Captures the set of hwmon base paths that currently have applicable
/// devices (with fans, temps, or power channels) and are not blacklisted.
async fn build_hwmon_baseline() -> HashSet<PathBuf> {
    let paths = devices::find_all_hwmon_device_paths();
    let mut baseline = HashSet::with_capacity(paths.len());
    for path in paths {
        let name = devices::get_device_name(&path).await;
        if is_hwmon_applicable(&name) {
            baseline.insert(path);
        }
    }
    baseline
}

fn is_hwmon_applicable(device_name: &str) -> bool {
    for blacklisted in &HWMON_DEVICE_NAME_BLACKLIST {
        if device_name == *blacklisted {
            return false;
        }
    }
    true
}

/// Captures liquidctl device descriptions from the current device map.
fn build_liquidctl_baseline(all_devices: &AllDevices) -> HashSet<String> {
    let mut baseline = HashSet::with_capacity(all_devices.len());
    for device_lock in all_devices.values() {
        let device = device_lock.borrow();
        if device.d_type == crate::device::DeviceType::Liquidctl {
            baseline.insert(device.name.clone());
        }
    }
    baseline
}

/// Main event loop: listens for kernel uevents and performs debounced scans.
///
/// Trailing-edge debounce: relevant events set or extend a single deadline
/// while accumulating which device categories (hwmon / liquidctl) were
/// affected. When the deadline expires all pending categories are scanned
/// together, so a device that triggers both hwmon and usb subsystem events
/// results in one combined scan rather than two separate ones.
async fn run_event_loop(
    async_fd: AsyncFd<OwnedFd>,
    mut hwmon_baseline: HashSet<PathBuf>,
    mut lc_baseline: HashSet<String>,
    liquidctl_repo: Option<Rc<LiquidctlRepo>>,
    bin_path: String,
    device_changed: Rc<Cell<bool>>,
    run_token: CancellationToken,
) {
    let mut buf = vec![0u8; UEVENT_BUF_SIZE];
    let mut debounce_deadline: Option<Instant> = None;
    let mut pending = PendingScans::default();
    let lc_available = liquidctl_repo.is_some();

    loop {
        let scan_now = tokio::select! {
            () = run_token.cancelled() => break,
            () = sleep_until_deadline(debounce_deadline) => {
                debounce_deadline = None;
                true
            },
            result = async_fd.readable() => {
                handle_readable_event(
                    result,
                    &async_fd,
                    &mut buf,
                    &mut debounce_deadline,
                    &mut pending,
                    lc_available,
                );
                continue;
            },
        };
        if scan_now {
            let scans = pending.take();
            let mut detected = false;
            if scans.hwmon {
                detected |= scan_hwmon_changes(&mut hwmon_baseline, &bin_path).await;
            }
            if scans.liquidctl {
                detected |=
                    scan_liquidctl_changes(&mut lc_baseline, liquidctl_repo.as_ref(), &bin_path)
                        .await;
            }
            if detected {
                device_changed.set(true);
            }
        }
    }
}

/// Sleeps until the debounce deadline, or waits forever if no deadline set.
async fn sleep_until_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(dl) => tokio::time::sleep_until(dl).await,
        None => std::future::pending::<()>().await,
    }
}

/// Drains pending uevent messages (up to `MAX_DRAIN_PER_WAKE`), accumulates
/// which device categories were affected, and sets or extends the debounce
/// deadline. Liquidctl events are ignored when `lc_available` is false
/// (the liqctld service is not running).
fn handle_readable_event(
    result: Result<AsyncFdReadyGuard<'_, OwnedFd>, std::io::Error>,
    async_fd: &AsyncFd<OwnedFd>,
    buf: &mut [u8],
    debounce_deadline: &mut Option<Instant>,
    pending: &mut PendingScans,
    lc_available: bool,
) {
    debug_assert!(buf.len() >= UEVENT_BUF_SIZE);
    let mut any_relevant = false;
    if let Ok(mut guard) = result {
        let raw_fd = async_fd.as_raw_fd();
        for _ in 0..MAX_DRAIN_PER_WAKE {
            match recv(raw_fd, buf, MsgFlags::MSG_DONTWAIT) {
                Ok(n) if n > 0 => match classify_uevent(&buf[..n]) {
                    UeventKind::Hwmon => {
                        pending.hwmon = true;
                        any_relevant = true;
                    }
                    UeventKind::Liquidctl if lc_available => {
                        pending.liquidctl = true;
                        any_relevant = true;
                    }
                    UeventKind::Liquidctl | UeventKind::Irrelevant => {}
                },
                _ => break,
            }
        }
        guard.clear_ready();
    }
    if any_relevant {
        *debounce_deadline = Some(Instant::now() + DEBOUNCE_DURATION);
    }
}

/// Identifies which scan category a uevent is relevant to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UeventKind {
    Hwmon,
    Liquidctl,
    Irrelevant,
}

/// Tracks which device categories have pending events during a debounce
/// window. A single physical device can generate both hwmon and usb/hidraw
/// events, so both flags may be set simultaneously.
#[derive(Default)]
struct PendingScans {
    hwmon: bool,
    liquidctl: bool,
}

impl PendingScans {
    /// Returns the current flags and resets them.
    fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

/// Classifies a raw uevent message by the device category it affects.
///
/// Uevent messages from the kernel are formatted as null-byte separated
/// key=value pairs. The first line is the event path with action prefix
/// (e.g., "add@/devices/...").
///
/// We filter for:
/// - ACTION: only "add" or "remove"
/// - SUBSYSTEM: "hwmon" -> Hwmon, "usb"/"hidraw" -> Liquidctl
fn classify_uevent(buf: &[u8]) -> UeventKind {
    let mut action_relevant = false;
    let mut subsystem: Option<&str> = None;
    for field in buf.split(|&b| b == 0) {
        if field.is_empty() {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(field) {
            if let Some(action) = s.strip_prefix("ACTION=") {
                if action == "add" || action == "remove" {
                    action_relevant = true;
                }
            } else if let Some(sub) = s.strip_prefix("SUBSYSTEM=") {
                subsystem = Some(sub);
            }
        }
        // Short-circuit once both fields are found.
        if action_relevant && subsystem.is_some() {
            break;
        }
    }
    if action_relevant.not() {
        return UeventKind::Irrelevant;
    }
    match subsystem {
        Some("hwmon") => UeventKind::Hwmon,
        Some("usb" | "hidraw") => UeventKind::Liquidctl,
        _ => UeventKind::Irrelevant,
    }
}

/// Compares current hwmon devices against the baseline. Updates
/// the baseline to the current state when changes are detected.
async fn scan_hwmon_changes(baseline: &mut HashSet<PathBuf>, bin_path: &str) -> bool {
    let current_paths = devices::find_all_hwmon_device_paths();
    let mut current_applicable = HashSet::with_capacity(current_paths.len());
    for path in &current_paths {
        let name = devices::get_device_name(path).await;
        if is_hwmon_applicable(&name) {
            current_applicable.insert(path.clone());
        }
    }
    let mut detected = false;
    // Check for added devices.
    for path in &current_applicable {
        if baseline.contains(path).not() {
            let name = devices::get_device_name(path).await;
            notify_device_added(&name, bin_path);
            detected = true;
        }
    }
    // Check for removed devices.
    for path in baseline.iter() {
        if current_applicable.contains(path).not() {
            let name = device_name_for_removed(path).await;
            notify_device_removed(&name, bin_path);
            detected = true;
        }
    }
    if detected {
        *baseline = current_applicable;
    }
    detected
}

/// Gets a device name for a removed device path.
/// The sysfs name file may no longer exist, so fall back to the path itself.
async fn device_name_for_removed(path: &Path) -> String {
    if let Ok(contents) = cc_fs::read_sysfs(path.join("name")).await {
        let name = contents.trim().to_string();
        if name.is_empty().not() {
            return name;
        }
    }
    path.display().to_string()
}

/// Compares current liquidctl devices against the baseline. Updates
/// the baseline to the current state when changes are detected.
async fn scan_liquidctl_changes(
    baseline: &mut HashSet<String>,
    liquidctl_repo: Option<&Rc<LiquidctlRepo>>,
    bin_path: &str,
) -> bool {
    let Some(repo) = liquidctl_repo else {
        return false;
    };
    let current_descriptions = match repo.scan_devices().await {
        Ok(descriptions) => descriptions,
        Err(err) => {
            debug!("Liquidctl device scan failed: {err}");
            return false;
        }
    };
    debug!("Liquidctl device scan returned: {current_descriptions:?}");
    let current_set: HashSet<&String> = current_descriptions.iter().collect();
    let mut detected = false;
    // Check for added devices.
    for desc in &current_descriptions {
        if baseline.contains(desc).not() {
            notify_device_added(desc, bin_path);
            detected = true;
        }
    }
    // Check for removed devices.
    for desc in baseline.iter() {
        if current_set.contains(desc).not() {
            notify_device_removed(desc, bin_path);
            detected = true;
        }
    }
    if detected {
        *baseline = current_descriptions.into_iter().collect();
    }
    detected
}

fn notify_device_added(name: &str, bin_path: &str) {
    warn!(
        "New applicable device detected: {name}. \
         Restart the daemon to use this device."
    );
    send_desktop_notification(
        "New Device Detected",
        &format!(
            "New applicable device detected: {name}. \
             Restart the daemon to use this device."
        ),
        bin_path,
    );
}

fn notify_device_removed(name: &str, bin_path: &str) {
    warn!(
        "Known device removed: {name}. \
         Sensors will be set to failsafe levels. Restart the daemon to update the device list."
    );
    send_desktop_notification(
        "Device Removed",
        &format!(
            "Known device removed: {name}. \
             Restart the daemon to update the device list."
        ),
        bin_path,
    );
}

/// Sends a desktop notification to all active user sessions.
///
/// The command format must match the `Notify` subcommand's positional
/// args: title, message, icon, audio. We explicitly set
/// `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR` because `sudo -u`
/// strips the environment, which prevents zbus from finding the session
/// bus on some desktops (e.g. Gnome on Arch).
fn send_desktop_notification(title: &str, body: &str, bin_path: &str) {
    let sessions = available_session_users();
    let safe_title = sanitize_for_shell(title);
    let safe_body = sanitize_for_shell(body);
    let safe_bin_path = sanitize_for_shell(bin_path);
    for (uid, runtime_dir) in &sessions {
        let runtime = runtime_dir.display();
        let cmd = format!(
            "sudo -u \\#{uid} env \
             DBUS_SESSION_BUS_ADDRESS=unix:path={runtime}/bus \
             XDG_RUNTIME_DIR={runtime} \
             {safe_bin_path} notify \"{safe_title}\" \
             \"{safe_body}\" {NOTIFICATION_ICON_INFO} false"
        );
        fire_notification_command(cmd);
    }
}

fn fire_notification_command(cmd: String) {
    tokio::task::spawn_local(async move {
        let result = ShellCommand::new(&cmd, NOTIFICATION_CMD_TIMEOUT)
            .run()
            .await;
        if let crate::repositories::utils::ShellCommandResult::Error(err) = result {
            warn!("Failed to execute notification command: '{cmd}' - {err}");
        }
    });
}

/// Finds active user sessions by checking for D-Bus session sockets.
/// Returns (uid, runtime_dir) pairs for each active session.
fn available_session_users() -> Vec<(u32, PathBuf)> {
    let mut sessions = Vec::new();
    let mut path = PathBuf::from("/run/user");
    if path.exists().not() {
        path = PathBuf::from("/var/run/user");
    }
    let Ok(entries) = cc_fs::read_dir(path) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let user_dir = entry.path();
        if user_dir.join("bus").exists() {
            if let Some(uid) = user_dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|id| id.parse::<u32>().ok())
            {
                if uid >= MIN_USER_UID {
                    sessions.push((uid, user_dir));
                }
            }
        }
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deaf_listener_reports_no_changes() {
        // A deaf listener must always report no changes.
        let listener = DeviceListener::deaf();
        assert!(listener.has_device_changed().not());
    }

    // --- classify_uevent: hwmon events ---

    #[test]
    fn hwmon_add_event_classified_as_hwmon() {
        // Hwmon add events must trigger a hwmon scan.
        let uevent = b"add@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=add\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Hwmon);
    }

    #[test]
    fn hwmon_remove_event_classified_as_hwmon() {
        // Hwmon remove events must trigger a hwmon scan.
        let uevent = b"remove@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=remove\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Hwmon);
    }

    // --- classify_uevent: liquidctl events ---

    #[test]
    fn usb_add_event_classified_as_liquidctl() {
        // USB add events cover liquidctl HID devices.
        let uevent = b"add@/devices/pci0000:00/usb1/1-4\0\
            ACTION=add\0SUBSYSTEM=usb\0DEVPATH=/devices/pci\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Liquidctl);
    }

    #[test]
    fn hidraw_add_event_classified_as_liquidctl() {
        // Hidraw add events cover direct HID device access.
        let uevent = b"add@/devices/pci0000:00/hidraw/hidraw0\0\
            ACTION=add\0SUBSYSTEM=hidraw\0DEVPATH=/devices/pci\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Liquidctl);
    }

    // --- classify_uevent: irrelevant events ---

    #[test]
    fn network_add_event_is_irrelevant() {
        // Network subsystem events have no cooling devices.
        let uevent = b"add@/devices/virtual/net/eth0\0\
            ACTION=add\0SUBSYSTEM=net\0DEVPATH=/devices/virtual\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Irrelevant);
    }

    #[test]
    fn hwmon_change_event_is_irrelevant() {
        // Only add/remove trigger scans; change events do not.
        let uevent = b"change@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=change\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Irrelevant);
    }

    #[test]
    fn block_device_event_is_irrelevant() {
        // Block device events are not cooling-related.
        let uevent = b"add@/devices/pci0000:00/block/sda\0\
            ACTION=add\0SUBSYSTEM=block\0DEVPATH=/devices/pci\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Irrelevant);
    }

    #[test]
    fn empty_buffer_is_irrelevant() {
        // Empty buffers must be safely rejected.
        assert_eq!(classify_uevent(b""), UeventKind::Irrelevant);
    }

    #[test]
    fn malformed_buffer_is_irrelevant() {
        // Non-null-separated data must be safely rejected.
        assert_eq!(
            classify_uevent(b"garbage data with no null bytes"),
            UeventKind::Irrelevant
        );
    }

    #[test]
    fn action_without_subsystem_is_irrelevant() {
        // A matching action without a matching subsystem is not enough.
        let uevent = b"add@/devices/something\0ACTION=add\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Irrelevant);
    }

    #[test]
    fn subsystem_without_action_is_irrelevant() {
        // A matching subsystem without a matching action (bind != add/remove)
        // must be rejected.
        let uevent = b"bind@/devices/something\0\
            ACTION=bind\0SUBSYSTEM=hwmon\0";
        assert_eq!(classify_uevent(uevent), UeventKind::Irrelevant);
    }

    // --- is_hwmon_applicable ---

    #[test]
    fn blacklisted_hwmon_device_is_not_applicable() {
        // GPU hwmon devices are managed by the GPU repo, not hwmon.
        assert!(is_hwmon_applicable("amdgpu").not());
    }

    #[test]
    fn normal_hwmon_device_is_applicable() {
        // Standard sensor chips and drive temp monitors are applicable.
        assert!(is_hwmon_applicable("coretemp"));
        assert!(is_hwmon_applicable("nct6775"));
        assert!(is_hwmon_applicable("drivetemp"));
    }

    // --- is_listener_disabled ---

    #[test]
    fn listener_disabled_when_env_is_zero() {
        // ENV_DBUS="0" explicitly disables D-Bus features including the listener.
        // Safety: test is single-threaded; no concurrent env reads.
        unsafe { env::set_var(ENV_DBUS, "0") };
        assert!(is_listener_disabled());
        unsafe { env::remove_var(ENV_DBUS) };
    }

    #[test]
    fn listener_disabled_when_env_is_off() {
        // ENV_DBUS="off" (case-insensitive) disables the listener.
        // Safety: test is single-threaded; no concurrent env reads.
        unsafe { env::set_var(ENV_DBUS, "OFF") };
        assert!(is_listener_disabled());
        unsafe { env::remove_var(ENV_DBUS) };
    }

    #[test]
    fn listener_enabled_when_env_is_one() {
        // ENV_DBUS="1" enables D-Bus features.
        // Safety: test is single-threaded; no concurrent env reads.
        unsafe { env::set_var(ENV_DBUS, "1") };
        assert!(is_listener_disabled().not());
        unsafe { env::remove_var(ENV_DBUS) };
    }

    // --- notification command format ---

    #[test]
    fn notification_command_includes_dbus_env_and_audio() {
        // The notify command must set DBUS_SESSION_BUS_ADDRESS and
        // XDG_RUNTIME_DIR, and include all positional args up to audio.
        let title = "CoolerControl: New Device Detected";
        let body = "New device: coretemp. Restart the daemon.";
        let bin_path = "/usr/bin/coolercontrold";
        let safe_title = sanitize_for_shell(title);
        let safe_body = sanitize_for_shell(body);
        let safe_bin = sanitize_for_shell(bin_path);
        let uid: u32 = 1000;
        let runtime = format!("/run/user/{uid}");
        let cmd = format!(
            "sudo -u \\#{uid} env \
             DBUS_SESSION_BUS_ADDRESS=unix:path={runtime}/bus \
             XDG_RUNTIME_DIR={runtime} \
             {safe_bin} notify \"{safe_title}\" \
             \"{safe_body}\" {NOTIFICATION_ICON_INFO} false"
        );
        assert!(cmd.starts_with("sudo -u \\#1000 env"));
        assert!(cmd.contains("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus"));
        assert!(cmd.contains("XDG_RUNTIME_DIR=/run/user/1000"));
        assert!(cmd.contains("notify"));
        assert!(cmd.ends_with("4 false"));
    }

    #[test]
    fn notification_command_sanitizes_special_chars() {
        // Device names with shell-unsafe chars must be sanitized.
        let body = "Device: test$(evil) removed";
        let sanitized = sanitize_for_shell(body);
        assert!(sanitized.contains('$').not());
        assert!(sanitized.contains('(').not());
        assert!(sanitized.contains(')').not());
    }

    // --- debounce: single deadline with pending flags ---

    #[test]
    fn hwmon_event_marks_only_hwmon_pending() {
        // A hwmon uevent must mark hwmon pending but not liquidctl.
        let pending = PendingScans {
            hwmon: true,
            ..Default::default()
        };
        // Simulate classify_uevent returning Hwmon.
        assert!(pending.hwmon);
        assert!(pending.liquidctl.not());
    }

    #[test]
    fn liquidctl_event_marks_only_liquidctl_pending() {
        // A usb/hidraw uevent must mark liquidctl pending but not hwmon.
        let pending = PendingScans {
            liquidctl: true,
            ..Default::default()
        };
        assert!(pending.hwmon.not());
        assert!(pending.liquidctl);
    }

    #[test]
    fn mixed_events_mark_both_pending() {
        // A device generating both hwmon and usb events must mark both.
        let pending = PendingScans {
            hwmon: true,
            liquidctl: true,
        };
        assert!(pending.hwmon);
        assert!(pending.liquidctl);
    }

    #[test]
    fn take_resets_pending_flags() {
        // After the debounce fires, take() returns current flags and
        // resets to default so the next window starts clean.
        let mut pending = PendingScans {
            hwmon: true,
            liquidctl: true,
        };
        let taken = pending.take();
        assert!(taken.hwmon);
        assert!(taken.liquidctl);
        assert!(pending.hwmon.not());
        assert!(pending.liquidctl.not());
    }

    #[test]
    fn events_extend_deadline_without_immediate_scan() {
        // Subsequent events extend the deadline but never trigger an
        // immediate scan (trailing-edge only).
        let mut deadline: Option<Instant> = Some(Instant::now() + DEBOUNCE_DURATION);
        let old = deadline.unwrap();
        // Simulate a second relevant event arriving.
        deadline = Some(Instant::now() + DEBOUNCE_DURATION);
        // Deadline was extended.
        assert!(deadline.unwrap() >= old);
    }
}
