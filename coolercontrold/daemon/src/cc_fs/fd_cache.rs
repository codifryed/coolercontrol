// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Holds sysfs descriptors open across ticks, indexed by slot.
//!
//! Reopening an attribute every tick is 83-95% of the cost of a cheap sensor read. A held
//! descriptor still re-invokes the driver's `show()` on each read at offset 0, so values stay live.
//!
//! The table is installed once, after detection has settled a device's channel set, and the
//! per-tick callers address it by slot. Nothing is hashed and no path crosses a thread boundary
//! after init. Only per-tick numeric reads belong here: a user-supplied file can be replaced with
//! a new inode, which a held descriptor would never see.

use anyhow::{anyhow, Result};
use std::path::Path;

use super::SysfsValue;

use super::{
    log_reissue, log_reissue_exhausted, reissue_backoff, should_reissue, INTERRUPTED_READ_ATTEMPTS,
    SYSFS_VALUE_MAX_BYTES,
};
use crate::rt;
use nix::libc;
use std::cell::RefCell;
use std::ops::Not;
use std::path::PathBuf;
use std::rc::Rc;

/// A slot in a device's read table. `u16` because a device has tens of attributes, not thousands.
pub type ReadIndex = u16;

/// One attribute's slot: the path it was registered with, and its descriptor once opened.
#[derive(Debug)]
struct Entry {
    path: PathBuf,
    file: Option<compio::fs::File>,
}

/// Descriptors held open for one device's per-tick attributes, addressed by slot.
///
/// Cloning shares one table (the handle is an `Rc`), so a device's table follows its driver info.
/// Dropping the last clone closes every descriptor, and compio's close needs no live runtime.
#[derive(Clone, Debug, Default)]
pub struct SysfsFdCache {
    entries: Rc<RefCell<Vec<Entry>>>,
}

impl SysfsFdCache {
    /// Installs the device's read table. Called once, after detection settles the channel set.
    ///
    /// Replacing a populated table drops every descriptor it held, which is what makes this safe
    /// to call again on a re-detect even though nothing does today.
    pub fn install(&self, paths: Vec<PathBuf>) {
        *self.entries.borrow_mut() = paths
            .into_iter()
            .map(|path| Entry { path, file: None })
            .collect();
    }

    /// Reads one registered attribute, reusing its descriptor when there is one.
    ///
    /// # Errors
    ///
    /// When the slot was never registered, or the read fails.
    pub async fn read_index(&self, index: ReadIndex) -> Result<SysfsValue> {
        let Some((path, held)) = self.slot(index) else {
            return Err(anyhow!("sysfs read slot {index} was never registered"));
        };
        if let Some(file) = held {
            return match Self::read_at_start(&path, &file).await {
                Ok(value) => Ok(value),
                Err(err) => {
                    if Self::is_dead_descriptor(&err) {
                        self.release(index);
                    }
                    Err(err)
                }
            };
        }
        let file = compio::fs::File::open(&path).await?;
        self.hold(index, &file);
        Self::read_at_start(&path, &file).await
    }

    /// The path a slot was registered with, for logs and for the caller's own assertions.
    #[must_use]
    pub fn path_of(&self, index: ReadIndex) -> Option<PathBuf> {
        self.entries
            .borrow()
            .get(index as usize)
            .map(|entry| entry.path.clone())
    }

    /// Releases every held descriptor, keeping the table. Used when the devices behind them may be
    /// gone (suspend).
    pub fn clear(&self) {
        for entry in self.entries.borrow_mut().iter_mut() {
            entry.file = None;
        }
    }

    /// Number of descriptors currently held open.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .borrow()
            .iter()
            .filter(|entry| entry.file.is_some())
            .count()
    }

    /// Only tests call this today; it exists because a pub `len` demands it (clippy).
    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cloning the handle (an `Rc` bump) ends the borrow before any await point.
    fn slot(&self, index: ReadIndex) -> Option<(PathBuf, Option<compio::fs::File>)> {
        self.entries
            .borrow()
            .get(index as usize)
            .map(|entry| (entry.path.clone(), entry.file.clone()))
    }

    fn hold(&self, index: ReadIndex, file: &compio::fs::File) {
        if let Some(entry) = self.entries.borrow_mut().get_mut(index as usize) {
            entry.file = Some(file.clone());
        }
    }

    fn release(&self, index: ReadIndex) {
        if let Some(entry) = self.entries.borrow_mut().get_mut(index as usize) {
            entry.file = None;
        }
    }

    /// One read at offset 0. kernfs regenerates the whole attribute per read, so a second read
    /// only ever reports 0 bytes; a value that fills the buffer is rejected by
    /// `SysfsValue::parse` as possibly truncated, exactly as on the uncached path.
    async fn read_at_start(path: &Path, file: &compio::fs::File) -> Result<SysfsValue> {
        use compio::buf::{IntoInner, IoBuf};
        use compio::io::AsyncReadAt;
        let mut buf = [0u8; SYSFS_VALUE_MAX_BYTES];
        let mut attempts = INTERRUPTED_READ_ATTEMPTS;
        // Bounded by `attempts`, which drops by one per failure and returns the error at zero.
        // See `INTERRUPTED_READ_ATTEMPTS` for why a read the kernel aborted is ours to re-issue.
        let err = loop {
            let compio::BufResult(result, slice) = file.read_at(buf.slice(..), 0).await;
            buf = slice.into_inner();
            match result {
                Ok(len) => {
                    debug_assert!(len <= SYSFS_VALUE_MAX_BYTES);
                    return Ok(SysfsValue::from_read(buf, len));
                }
                Err(err) => {
                    debug_assert!(attempts > 0);
                    attempts -= 1;
                    if should_reissue(attempts, &err).not() {
                        if err.kind() == std::io::ErrorKind::Interrupted {
                            log_reissue_exhausted(path);
                        }
                        break err;
                    }
                    log_reissue(path, attempts);
                    rt::sleep(reissue_backoff(attempts)).await;
                }
            }
        };
        Err(err.into())
    }

    /// Errnos that mean the descriptor itself is dead: the device was unbound or removed, or the
    /// path was replaced. Value-level failures (EPERM from a runtime-suspended GPU, ENODATA from
    /// an unconnected sensor slot) leave the descriptor usable, and evicting on those would
    /// reopen every tick, which is the cost this cache exists to remove.
    fn is_dead_descriptor(err: &anyhow::Error) -> bool {
        err.downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error)
            .is_some_and(|errno| {
                matches!(
                    errno,
                    libc::ENODEV | libc::ESTALE | libc::EBADF | libc::ENOENT
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    /// Goal: a held descriptor must report the file's current contents, not the bytes seen when it
    /// was opened. This is the whole premise of the cache: kernfs regenerates an attribute on
    /// every read at offset 0. Method: read a value through the cache, rewrite the same inode, and
    /// read again through the same cache.
    #[test]
    fn held_descriptor_sees_rewritten_values() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("temp1_input");
            let cache = SysfsFdCache::default();
            cache.install(vec![path.clone()]);
            for expected in ["45000", "31000", "0", "120000"] {
                std::fs::write(&path, format!("{expected}\n")).unwrap();
                let value = cache.read_index(0).await.unwrap();
                assert_eq!(value.trimmed_str().unwrap(), expected);
            }
        });
    }

    /// Goal: repeated reads of one slot must reuse a single descriptor, and separate slots must
    /// each get their own. Method: read one slot twice and a second slot once, asserting the
    /// held count after each step.
    #[test]
    fn each_slot_holds_exactly_one_descriptor() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let first = dir.path().join("temp1_input");
            let second = dir.path().join("fan1_input");
            std::fs::write(&first, "45000\n").unwrap();
            std::fs::write(&second, "1200\n").unwrap();
            let cache = SysfsFdCache::default();
            cache.install(vec![first, second]);
            assert!(cache.is_empty());
            cache.read_index(0).await.unwrap();
            assert_eq!(cache.len(), 1);
            cache.read_index(0).await.unwrap();
            assert_eq!(cache.len(), 1, "second read of one slot opened again");
            cache.read_index(1).await.unwrap();
            assert_eq!(cache.len(), 2);
        });
    }

    /// Goal: `clear` must release every descriptor, and reads after it must still work by
    /// reopening. Method: populate the cache, clear it, then read again and check the value and
    /// the held count.
    #[test]
    fn clear_releases_descriptors_and_reads_still_work() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("temp1_input");
            std::fs::write(&path, "45000\n").unwrap();
            let cache = SysfsFdCache::default();
            cache.install(vec![path]);
            cache.read_index(0).await.unwrap();
            assert_eq!(cache.len(), 1);
            cache.clear();
            assert!(cache.is_empty());
            let value = cache.read_index(0).await.unwrap();
            assert_eq!(value.trimmed_str().unwrap(), "45000");
            assert_eq!(cache.len(), 1);
        });
    }

    /// Goal: eviction must force the next read to reopen, which is how a dead descriptor recovers.
    /// Method: read a path, evict it, then read again and assert the value is correct and exactly
    /// one descriptor is held.
    #[test]
    fn eviction_forces_a_reopen() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("temp1_input");
            std::fs::write(&path, "45000\n").unwrap();
            let cache = SysfsFdCache::default();
            cache.install(vec![path.clone()]);
            cache.read_index(0).await.unwrap();
            cache.release(0);
            assert!(cache.is_empty());
            std::fs::write(&path, "46000\n").unwrap();
            let value = cache.read_index(0).await.unwrap();
            assert_eq!(value.trimmed_str().unwrap(), "46000");
            assert_eq!(cache.len(), 1);
        });
    }

    /// Goal: only errnos that mean a dead descriptor may evict. Evicting on a value-level failure
    /// would reopen every tick for a suspended GPU or an unconnected sensor slot, undoing the
    /// cache. Method: classify one error per errno, plus a non-io error.
    #[test]
    fn only_dead_descriptor_errnos_evict() {
        for errno in [libc::ENODEV, libc::ESTALE, libc::EBADF, libc::ENOENT] {
            let err: anyhow::Error = std::io::Error::from_raw_os_error(errno).into();
            assert!(
                SysfsFdCache::is_dead_descriptor(&err),
                "errno {errno} should evict"
            );
        }
        for errno in [libc::EPERM, libc::EIO, libc::ENODATA, libc::EAGAIN] {
            let err: anyhow::Error = std::io::Error::from_raw_os_error(errno).into();
            assert!(
                SysfsFdCache::is_dead_descriptor(&err).not(),
                "errno {errno} must keep the descriptor"
            );
        }
        let parse_err: anyhow::Error = anyhow::anyhow!("invalid digit found in string");
        assert!(SysfsFdCache::is_dead_descriptor(&parse_err).not());
    }

    /// Goal: a failed open must not leave anything held, so a missing attribute costs one failed
    /// open per tick and nothing else. Method: read a path that does not exist.
    #[test]
    fn a_missing_path_holds_nothing() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cache = SysfsFdCache::default();
            cache.install(vec![dir.path().join("absent_input")]);
            let result = cache.read_index(0).await;
            assert!(result.is_err());
            assert!(cache.is_empty());
        });
    }

    /// Goal: document the limitation that keeps user files off this path. A descriptor follows the
    /// inode, so a file replaced by rename (what editors and scripts do) is invisible to the
    /// cache, while a sysfs attribute cannot be replaced that way. Method: read, replace the path
    /// with a different inode, read again, and assert the stale value.
    #[test]
    fn a_replaced_inode_is_not_seen() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("user_value");
            let replacement = dir.path().join("user_value.new");
            std::fs::write(&path, "45000\n").unwrap();
            let cache = SysfsFdCache::default();
            cache.install(vec![path.clone()]);
            assert_eq!(
                cache.read_index(0).await.unwrap().trimmed_str().unwrap(),
                "45000"
            );
            std::fs::write(&replacement, "99000\n").unwrap();
            std::fs::rename(&replacement, &path).unwrap();
            assert_eq!(
                cache.read_index(0).await.unwrap().trimmed_str().unwrap(),
                "45000",
                "a held descriptor must not be used for files that can be replaced"
            );
        });
    }
}
