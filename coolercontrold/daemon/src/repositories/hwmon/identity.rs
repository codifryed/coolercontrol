// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Identity reads during a repository's init, on a thread of their own.
//!
//! A device's identity attributes (its name, serial, wwid, model and `uevent`) decide the UID its
//! settings are keyed to. The driver core serves them from memory, but they are sysfs reads all
//! the same, so they go through a worker like every other read and none runs on the main thread.
//!
//! A read that gets no answer is not an absent attribute. An absent serial makes the UID fall back
//! to the device's location, as it always has; a stalled one must not, or the device's settings
//! are keyed to a device that does not exist. The device is skipped for this boot instead, and is
//! found again at the next start.
use std::cell::RefCell;
use std::fmt::Display;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::time::Duration;

use log::{debug, warn};

use crate::repositories::hwmon::device_io::{self, DeviceIo};

/// Devices one reader remembers as stalled, each with a worker parked in the kernel. Past this
/// every read is a stall: on a machine where this many devices hung, another worker would only
/// park as well.
const STALLED_DEVICES_MAX: usize = 32;

/// An identity read that got no answer. The device it was for is skipped for this boot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stalled;

/// Says a device was skipped because an identity read got no answer. A warning: the device is
/// missing from the app until the next start.
pub fn log_skipped(device: &dyn Display, path: &Path) {
    warn!(
        "Skipping {device} at {} for this start: reading its identity got no answer. It is read \
         again at the next start.",
        path.display()
    );
}

/// As `log_skipped`, for every device of one driver, when a read they all depend on, named by
/// `what`, got no answer.
pub fn log_driver_skipped(driver: &str, what: &str) {
    warn!(
        "Skipping every {driver} device for this start: reading {what} got no answer. It is read \
         again at the next start."
    );
}

/// Reads identity attributes for one init, or for the hotplug listener, on a worker of its own.
///
/// The worker starts on the first read, so an init whose attributes are all cached already (the
/// `uevent` cache) starts none, and it ends when the reader is dropped.
///
/// A stall parks the worker in the kernel, so the reader remembers that device and starts a fresh
/// worker for the next one. A device that stalled once is not read again: its reads stall at once,
/// so each wedged device costs one thread, and devices that answer are still read.
pub struct IdentityReader {
    /// `None` until the first read, and again after a stall, until the next.
    io: RefCell<Option<DeviceIo>>,
    /// Names the worker, `ccio-<owner>`.
    owner: &'static str,
    reply_timeout: Duration,
    /// Starts a worker. A field so tests can replace a wedged worker with one that answers,
    /// without starting a thread.
    start_worker: fn(&'static str, Duration) -> DeviceIo,
    /// Devices a read got no answer from, as `device_of` names them. At most
    /// `STALLED_DEVICES_MAX`.
    stalled: RefCell<Vec<PathBuf>>,
}

impl IdentityReader {
    /// A reader whose worker, `ccio-<owner>`, starts on the first read. Without a thread it reads
    /// inline, as `DeviceIo` does: the isolation is lost, not the devices.
    #[must_use]
    pub fn start(owner: &'static str, reply_timeout: Duration) -> Self {
        Self::over(None, owner, reply_timeout, DeviceIo::isolated_or_inline)
    }

    fn over(
        io: Option<DeviceIo>,
        owner: &'static str,
        reply_timeout: Duration,
        start_worker: fn(&'static str, Duration) -> DeviceIo,
    ) -> Self {
        debug_assert!(reply_timeout > Duration::ZERO);
        debug_assert!(owner.is_empty().not());
        Self {
            io: RefCell::new(io),
            owner,
            reply_timeout,
            start_worker,
            stalled: RefCell::new(Vec::new()),
        }
    }

    /// Reads on the caller's runtime, for tests.
    #[cfg(test)]
    #[must_use]
    pub fn inline() -> Self {
        Self::over(None, "test", Duration::from_secs(1), |_, _| {
            DeviceIo::default()
        })
    }

    /// Reads through `io`, and through `restart`'s workers after a stall, for tests.
    #[cfg(test)]
    #[must_use]
    pub fn with_io(io: DeviceIo, restart: fn(&'static str, Duration) -> DeviceIo) -> Self {
        Self::over(Some(io), "test", Duration::from_secs(1), restart)
    }

    /// The current worker, started if there is none. A clone, so no borrow is held across a read:
    /// a stall drops the worker.
    fn io(&self) -> DeviceIo {
        self.io
            .borrow_mut()
            .get_or_insert_with(|| (self.start_worker)(self.owner, self.reply_timeout))
            .clone()
    }

    /// One attribute as text. `Ok(None)` when it is absent or unreadable, which callers treat as
    /// they always have.
    ///
    /// # Errors
    ///
    /// `Stalled` when the worker gave no answer.
    pub async fn text(&self, path: &Path) -> Result<Option<String>, Stalled> {
        if self.is_stalled(path) {
            return Err(Stalled);
        }
        match self.io().read_text(path).await {
            Ok(text) => Ok(Some(text)),
            Err(err) => self.unread(path, &err),
        }
    }

    /// One attribute as raw bytes, for binary pages such as SCSI VPD. As `text` otherwise.
    ///
    /// # Errors
    ///
    /// `Stalled` when the worker gave no answer.
    pub async fn bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, Stalled> {
        if self.is_stalled(path) {
            return Err(Stalled);
        }
        match self.io().read_bytes(path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) => self.unread(path, &err),
        }
    }

    /// Sorts a failed read into absent or stalled. An absent attribute is the common case and
    /// stays quiet; the caller names the device a stall skips.
    fn unread<T>(&self, path: &Path, err: &anyhow::Error) -> Result<Option<T>, Stalled> {
        if device_io::is_unanswered(err).not() {
            return Ok(None);
        }
        debug!(
            "{} identity read of {} got no answer: {err}",
            self.owner,
            path.display()
        );
        self.record_stall(path);
        Err(Stalled)
    }

    /// Whether `path` belongs to a device that stalled, or the reader has given up on new ones.
    fn is_stalled(&self, path: &Path) -> bool {
        let stalled = self.stalled.borrow();
        debug_assert!(stalled.len() <= STALLED_DEVICES_MAX);
        if stalled.len() == STALLED_DEVICES_MAX {
            return true;
        }
        let device = device_of(path);
        stalled.iter().any(|stalled| device.starts_with(stalled))
    }

    /// Remembers the device, and drops the worker parked in the kernel so the next read starts a
    /// fresh one.
    fn record_stall(&self, path: &Path) {
        let mut stalled = self.stalled.borrow_mut();
        // Reads in flight together can each stall past the check in `is_stalled`.
        if stalled.len() < STALLED_DEVICES_MAX {
            stalled.push(device_of(path).to_path_buf());
        }
        debug_assert!(stalled.len() <= STALLED_DEVICES_MAX);
        // Dropping the handle closes the queue, so the worker exits once its read returns.
        *self.io.borrow_mut() = None;
    }

    /// Forgets the stalled devices outside `present`, so a device that reappears at a reused path
    /// is read again. For a reader that outlives the devices it reads, such as hotplug's.
    pub fn forget_stalled_except(&self, present: &[PathBuf]) {
        self.stalled.borrow_mut().retain(|stalled| {
            present
                .iter()
                .any(|path| device_of(&path.join("name")).starts_with(stalled))
        });
    }
}

/// The device an attribute belongs to: its directory, or the one above for an attribute behind a
/// `device` link, so `hwmonN/name` and `hwmonN/device/serial` name the same device.
fn device_of(attribute: &Path) -> &Path {
    let dir = attribute.parent().unwrap_or(attribute);
    if dir.ends_with("device") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEDGE_TIMEOUT: Duration = Duration::from_millis(20);

    /// A worker whose queue is already closed, so every read fails at once as unanswered.
    fn gone(_owner: &'static str, reply_timeout: Duration) -> DeviceIo {
        DeviceIo::wedged_for_test(reply_timeout).0
    }

    #[test]
    fn an_answered_read_is_its_contents_and_an_absent_one_is_none() {
        // Goal: an attribute that is there reads as its contents, and one that is absent or not
        // text reads as `None`, exactly as callers treated a failed read before. Method: read a
        // text file, a binary file and a missing one inline.
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("serial"), "S5GXNX0R\n").unwrap();
            std::fs::write(dir.path().join("vpd_pg80"), [0x80, 0x80, 0x00, 0x01]).unwrap();
            let reader = IdentityReader::inline();

            let serial = reader.text(&dir.path().join("serial")).await;
            let page_as_text = reader.text(&dir.path().join("vpd_pg80")).await;
            let page = reader.bytes(&dir.path().join("vpd_pg80")).await;
            let absent = reader.text(&dir.path().join("wwid")).await;

            assert_eq!(serial, Ok(Some("S5GXNX0R\n".to_owned())));
            assert_eq!(page_as_text, Ok(None));
            assert_eq!(page, Ok(Some(vec![0x80, 0x80, 0x00, 0x01])));
            assert_eq!(absent, Ok(None));
        });
    }

    #[test]
    fn a_reader_starts_no_worker_until_its_first_read() {
        // Goal: an init whose attributes are all cached starts no thread. Method: create a reader,
        // check it holds no worker, then read once and check it does.
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("name"), "k10temp\n").unwrap();
            let reader = IdentityReader::inline();
            assert!(reader.io.borrow().is_none());

            let name = reader.text(&dir.path().join("name")).await;

            assert_eq!(name, Ok(Some("k10temp\n".to_owned())));
            assert!(reader.io.borrow().is_some());
        });
    }

    #[test]
    fn a_read_with_no_answer_stalls_and_the_next_device_gets_a_fresh_worker() {
        // Goal: a stall is reported as one, never as an absent attribute, and does not stall the
        // devices read after it. Method: read one device through a wedged worker whose
        // replacement answers, then another device.
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let other = dir.path().join("hwmon1");
            std::fs::create_dir(&other).unwrap();
            std::fs::write(other.join("name"), "nvme\n").unwrap();
            let (wedged, _rx) = DeviceIo::wedged_for_test(WEDGE_TIMEOUT);
            let reader = IdentityReader::with_io(wedged, |_, _| DeviceIo::default());

            let stalled = reader.text(&dir.path().join("hwmon0/serial")).await;
            let next = reader.text(&other.join("name")).await;

            assert_eq!(stalled, Err(Stalled));
            assert_eq!(next, Ok(Some("nvme\n".to_owned())));
            assert_eq!(reader.stalled.borrow().len(), 1);
        });
    }

    #[test]
    fn a_device_that_stalled_is_not_read_again() {
        // Goal: a wedged device costs one worker, not one per read: its later reads stall at
        // once, its `device` directory included, while another device is still read. Method:
        // stall one device, then read it again through workers that would answer.
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let device = dir.path().join("hwmon0");
            std::fs::create_dir_all(device.join("device")).unwrap();
            std::fs::write(device.join("name"), "nct6775\n").unwrap();
            std::fs::write(device.join("device/serial"), "S5GX\n").unwrap();
            let (wedged, _rx) = DeviceIo::wedged_for_test(WEDGE_TIMEOUT);
            let reader = IdentityReader::with_io(wedged, |_, _| DeviceIo::default());

            assert_eq!(reader.text(&device.join("uevent")).await, Err(Stalled));

            assert_eq!(reader.text(&device.join("name")).await, Err(Stalled));
            assert_eq!(
                reader.bytes(&device.join("device/serial")).await,
                Err(Stalled)
            );
            assert!(reader.io.borrow().is_none(), "no worker was started for it");
            assert_eq!(reader.stalled.borrow().len(), 1);
        });
    }

    #[test]
    fn a_reader_gives_up_once_too_many_devices_stalled() {
        // Goal: the parked workers are bounded, and once the limit is reached every read stalls
        // without waiting out another timeout. Method: replace a wedged worker with workers that
        // never answer, one stall per device up to the limit, then a device that would answer.
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("name"), "k10temp\n").unwrap();
            let (wedged, _rx) = DeviceIo::wedged_for_test(WEDGE_TIMEOUT);
            let reader = IdentityReader::with_io(wedged, gone);

            for index in 0..STALLED_DEVICES_MAX {
                let path = PathBuf::from(format!("/sys/class/hwmon/hwmon{index}/name"));
                assert_eq!(reader.text(&path).await, Err(Stalled));
            }

            assert_eq!(reader.stalled.borrow().len(), STALLED_DEVICES_MAX);
            assert_eq!(reader.text(&dir.path().join("name")).await, Err(Stalled));
        });
    }

    #[test]
    fn a_stalled_device_that_left_is_read_again() {
        // Goal: a long-lived reader forgets a stalled device once its path is gone, so a device
        // that reappears there is read, while one still present stays skipped. Method: stall two
        // devices, then forget all but one.
        crate::rt::test_runtime(async {
            let (wedged, _rx) = DeviceIo::wedged_for_test(WEDGE_TIMEOUT);
            let reader = IdentityReader::with_io(wedged, gone);
            let kept = PathBuf::from("/sys/class/hwmon/hwmon0");
            let gone_path = PathBuf::from("/sys/class/hwmon/hwmon1");
            assert_eq!(reader.text(&kept.join("name")).await, Err(Stalled));
            assert_eq!(reader.text(&gone_path.join("name")).await, Err(Stalled));

            reader.forget_stalled_except(&[kept.join("device")]);

            assert!(reader.is_stalled(&kept.join("name")));
            assert!(reader.is_stalled(&gone_path.join("name")).not());
        });
    }
}
