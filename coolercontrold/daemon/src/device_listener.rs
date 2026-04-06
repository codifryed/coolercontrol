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
use log::{debug, error, info, warn};
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
/// Leading-edge debounce: the first relevant event triggers an immediate
/// scan, then subsequent events within the debounce window are coalesced
/// into one trailing scan after the window expires.
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

    loop {
        let scan_needed = tokio::select! {
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
                )
            },
        };
        if scan_needed {
            if debounce_deadline.is_none() {
                // Set trailing debounce window for any follow-up events.
                debounce_deadline = Some(Instant::now() + DEBOUNCE_DURATION);
            }
            perform_scan(
                &mut hwmon_baseline,
                &mut lc_baseline,
                liquidctl_repo.as_ref(),
                &bin_path,
                &device_changed,
            )
            .await;
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

/// Drains pending uevent messages (up to `MAX_DRAIN_PER_WAKE`) and returns
/// whether a scan is needed.
fn handle_readable_event(
    result: Result<AsyncFdReadyGuard<'_, OwnedFd>, std::io::Error>,
    async_fd: &AsyncFd<OwnedFd>,
    buf: &mut [u8],
    debounce_deadline: &mut Option<Instant>,
) -> bool {
    debug_assert!(buf.len() >= UEVENT_BUF_SIZE);
    let mut relevant = false;
    if let Ok(mut guard) = result {
        let raw_fd = async_fd.as_raw_fd();
        for _ in 0..MAX_DRAIN_PER_WAKE {
            match recv(raw_fd, buf, MsgFlags::MSG_DONTWAIT) {
                Ok(n) if n > 0 => {
                    if is_relevant_uevent(&buf[..n]) {
                        relevant = true;
                    }
                }
                _ => break,
            }
        }
        guard.clear_ready();
    }
    if relevant {
        if debounce_deadline.is_none() {
            // Leading edge: trigger immediate scan.
            true
        } else {
            // Already debouncing: extend the window.
            *debounce_deadline = Some(Instant::now() + DEBOUNCE_DURATION);
            false
        }
    } else {
        false
    }
}

/// Checks whether a raw uevent message indicates a relevant device change.
///
/// Uevent messages from the kernel are formatted as null-byte separated
/// key=value pairs. The first line is the event path with action prefix
/// (e.g., "add@/devices/...").
///
/// We filter for:
/// - ACTION: only "add" or "remove"
/// - SUBSYSTEM: "hwmon", "usb", or "hidraw"
fn is_relevant_uevent(buf: &[u8]) -> bool {
    let mut action_relevant = false;
    let mut subsystem_relevant = false;
    for field in buf.split(|&b| b == 0) {
        if field.is_empty() {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(field) {
            if let Some(action) = s.strip_prefix("ACTION=") {
                if action == "add" || action == "remove" {
                    action_relevant = true;
                }
            } else if let Some(subsystem) = s.strip_prefix("SUBSYSTEM=") {
                if subsystem == "hwmon" || subsystem == "usb" || subsystem == "hidraw" {
                    subsystem_relevant = true;
                }
            }
        }
        // Short-circuit once both conditions are met.
        if action_relevant && subsystem_relevant {
            return true;
        }
    }
    false
}

/// Scans for device changes compared to the baselines. Updates
/// baselines to the current state so changes are only reported once.
async fn perform_scan(
    hwmon_baseline: &mut HashSet<PathBuf>,
    lc_baseline: &mut HashSet<String>,
    liquidctl_repo: Option<&Rc<LiquidctlRepo>>,
    bin_path: &str,
    device_changed: &Rc<Cell<bool>>,
) {
    debug!("Scanning for device changes...");
    let mut changes_detected = false;
    changes_detected |= scan_hwmon_changes(hwmon_baseline, bin_path).await;
    changes_detected |= scan_liquidctl_changes(lc_baseline, liquidctl_repo, bin_path).await;
    if changes_detected {
        device_changed.set(true);
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
        "CoolerControl: New Device Detected",
        &format!(
            "New applicable device detected: {name}. \
             Restart the daemon to use this device."
        ),
        bin_path,
    );
}

fn notify_device_removed(name: &str, bin_path: &str) {
    error!(
        "Known device removed: {name}. \
         Restart the daemon to update the device list."
    );
    send_desktop_notification(
        "CoolerControl: Device Removed",
        &format!(
            "Known device removed: {name}. \
             Restart the daemon to update the device list."
        ),
        bin_path,
    );
}

/// Sends a desktop notification to all active user sessions.
fn send_desktop_notification(title: &str, body: &str, bin_path: &str) {
    let user_ids = available_session_user_ids();
    let safe_title = sanitize_for_shell(title);
    let safe_body = sanitize_for_shell(body);
    let safe_bin_path = sanitize_for_shell(bin_path);
    for uid in &user_ids {
        let cmd = format!(
            "sudo -u \\#{uid} {safe_bin_path} notify \"{safe_title}\" \
             \"{safe_body}\" {NOTIFICATION_ICON_INFO}"
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
            debug!("Failed to execute notification command: {err}");
        }
    });
}

/// Finds active user session IDs by checking for D-Bus session sockets.
fn available_session_user_ids() -> HashSet<u32> {
    let mut user_ids = HashSet::new();
    let mut path = PathBuf::from("/run/user");
    if path.exists().not() {
        path = PathBuf::from("/var/run/user");
    }
    let Ok(entries) = cc_fs::read_dir(path) else {
        return user_ids;
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
                    user_ids.insert(uid);
                }
            }
        }
    }
    user_ids
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

    // --- is_relevant_uevent positive space: accepted events ---

    #[test]
    fn hwmon_add_event_is_relevant() {
        // Hwmon add events must trigger a device scan.
        let uevent = b"add@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=add\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert!(is_relevant_uevent(uevent));
    }

    #[test]
    fn hwmon_remove_event_is_relevant() {
        // Hwmon remove events must trigger a device scan.
        let uevent = b"remove@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=remove\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert!(is_relevant_uevent(uevent));
    }

    #[test]
    fn usb_add_event_is_relevant() {
        // USB add events cover liquidctl HID devices.
        let uevent = b"add@/devices/pci0000:00/usb1/1-4\0\
            ACTION=add\0SUBSYSTEM=usb\0DEVPATH=/devices/pci\0";
        assert!(is_relevant_uevent(uevent));
    }

    #[test]
    fn hidraw_add_event_is_relevant() {
        // Hidraw add events cover direct HID device access.
        let uevent = b"add@/devices/pci0000:00/hidraw/hidraw0\0\
            ACTION=add\0SUBSYSTEM=hidraw\0DEVPATH=/devices/pci\0";
        assert!(is_relevant_uevent(uevent));
    }

    // --- is_relevant_uevent negative space: rejected events ---

    #[test]
    fn network_add_event_is_not_relevant() {
        // Network subsystem events have no cooling devices.
        let uevent = b"add@/devices/virtual/net/eth0\0\
            ACTION=add\0SUBSYSTEM=net\0DEVPATH=/devices/virtual\0";
        assert!(is_relevant_uevent(uevent).not());
    }

    #[test]
    fn hwmon_change_event_is_not_relevant() {
        // Only add/remove trigger scans; change events do not.
        let uevent = b"change@/devices/platform/coretemp.0/hwmon/hwmon3\0\
            ACTION=change\0SUBSYSTEM=hwmon\0DEVPATH=/devices/platform\0";
        assert!(is_relevant_uevent(uevent).not());
    }

    #[test]
    fn block_device_event_is_not_relevant() {
        // Block device events are not cooling-related.
        let uevent = b"add@/devices/pci0000:00/block/sda\0\
            ACTION=add\0SUBSYSTEM=block\0DEVPATH=/devices/pci\0";
        assert!(is_relevant_uevent(uevent).not());
    }

    #[test]
    fn empty_buffer_is_not_relevant() {
        // Empty buffers must be safely rejected.
        assert!(is_relevant_uevent(b"").not());
    }

    #[test]
    fn malformed_buffer_is_not_relevant() {
        // Non-null-separated data must be safely rejected.
        assert!(is_relevant_uevent(b"garbage data with no null bytes").not());
    }

    #[test]
    fn action_without_subsystem_is_not_relevant() {
        // A matching action without a matching subsystem is not enough.
        let uevent = b"add@/devices/something\0ACTION=add\0";
        assert!(is_relevant_uevent(uevent).not());
    }

    #[test]
    fn subsystem_without_action_is_not_relevant() {
        // A matching subsystem without a matching action (bind != add/remove)
        // must be rejected.
        let uevent = b"bind@/devices/something\0\
            ACTION=bind\0SUBSYSTEM=hwmon\0";
        assert!(is_relevant_uevent(uevent).not());
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
}
