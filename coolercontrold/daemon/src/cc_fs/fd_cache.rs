// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Holds sysfs descriptors open across ticks.
//!
//! Reopening an attribute every tick is 83-95% of the cost of a cheap sensor read: the kernel
//! redoes the path walk and the kernfs open, and under `io_uring` every open and close is another
//! io-wq hand-off. A held descriptor still re-invokes the driver's `show()` on each read at offset
//! 0, so values stay live. Only per-tick numeric reads belong here: a user-supplied file can be
//! replaced with a new inode, which a held descriptor would never see.

use anyhow::Result;
use std::path::Path;

use super::SysfsValue;

use super::SYSFS_VALUE_MAX_BYTES;
use nix::libc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Upper bound on held descriptors per cache. One entry per per-tick attribute of one device, so
/// tens in practice. Past the bound reads fall back to open-per-read instead of growing unbounded.
pub const SYSFS_FD_CACHE_MAX_ENTRIES: usize = 512;

/// Descriptors held open for one device's per-tick attributes.
///
/// Cloning shares one cache (the handle is an `Rc`), so a device's cache follows its driver info.
/// Dropping the last clone closes every descriptor, and compio's close needs no live runtime.
#[derive(Clone, Debug, Default)]
pub struct SysfsFdCache {
    files: Rc<RefCell<HashMap<PathBuf, compio::fs::File>>>,
}

impl SysfsFdCache {
    /// Reads one numeric attribute, reusing a held descriptor when there is one.
    pub async fn read_value(&self, path: &Path) -> Result<SysfsValue> {
        {
            if let Some(file) = self.held(path) {
                return match Self::read_at_start(&file).await {
                    Ok(value) => Ok(value),
                    Err(err) => {
                        if Self::is_dead_descriptor(&err) {
                            self.evict(path);
                        }
                        Err(err)
                    }
                };
            }
            let file = compio::fs::File::open(path).await?;
            self.hold(path, &file);
            Self::read_at_start(&file).await
        }
    }

    /// Releases every held descriptor. Used when the devices behind them may be gone (suspend).
    pub fn clear(&self) {
        self.files.borrow_mut().clear();
    }

    /// Number of held descriptors. Always 0 on the tokio fallback, which holds none.
    #[must_use]
    pub fn len(&self) -> usize {
        {
            self.files.borrow().len()
        }
    }

    /// Only tests call this today; it exists because a pub `len` demands it (clippy).
    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cloning the handle (an `Rc` bump) ends the borrow before any await point.
    fn held(&self, path: &Path) -> Option<compio::fs::File> {
        self.files.borrow().get(path).cloned()
    }

    fn hold(&self, path: &Path, file: &compio::fs::File) {
        let mut files = self.files.borrow_mut();
        if files.len() >= SYSFS_FD_CACHE_MAX_ENTRIES {
            debug_assert!(
                false,
                "sysfs descriptor cache exceeded {SYSFS_FD_CACHE_MAX_ENTRIES} entries"
            );
            return;
        }
        files.insert(path.to_path_buf(), file.clone());
        debug_assert!(files.len() <= SYSFS_FD_CACHE_MAX_ENTRIES);
    }

    fn evict(&self, path: &Path) {
        self.files.borrow_mut().remove(path);
    }

    /// One read at offset 0. kernfs regenerates the whole attribute per read, so a second read
    /// only ever reports 0 bytes; a value that fills the buffer is rejected by
    /// `SysfsValue::parse` as possibly truncated, exactly as on the uncached path.
    async fn read_at_start(file: &compio::fs::File) -> Result<SysfsValue> {
        use compio::buf::{IntoInner, IoBuf};
        use compio::io::AsyncReadAt;
        let buf = [0u8; SYSFS_VALUE_MAX_BYTES];
        let compio::BufResult(result, slice) = file.read_at(buf.slice(..), 0).await;
        let len = result?;
        debug_assert!(len <= SYSFS_VALUE_MAX_BYTES);
        Ok(SysfsValue::from_read(slice.into_inner(), len))
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
            for expected in ["45000", "31000", "0", "120000"] {
                std::fs::write(&path, format!("{expected}\n")).unwrap();
                let value = cache.read_value(&path).await.unwrap();
                assert_eq!(value.trimmed_str().unwrap(), expected);
            }
        });
    }

    /// Goal: repeated reads of one path must reuse a single descriptor, and separate paths must
    /// each get their own. Method: read one path twice and a second path once, asserting the
    /// held count after each step.
    #[test]
    fn each_path_holds_exactly_one_descriptor() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let first = dir.path().join("temp1_input");
            let second = dir.path().join("fan1_input");
            std::fs::write(&first, "45000\n").unwrap();
            std::fs::write(&second, "1200\n").unwrap();
            let cache = SysfsFdCache::default();
            assert!(cache.is_empty());
            cache.read_value(&first).await.unwrap();
            assert_eq!(cache.len(), 1);
            cache.read_value(&first).await.unwrap();
            assert_eq!(cache.len(), 1, "second read of one path opened again");
            cache.read_value(&second).await.unwrap();
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
            cache.read_value(&path).await.unwrap();
            assert_eq!(cache.len(), 1);
            cache.clear();
            assert!(cache.is_empty());
            let value = cache.read_value(&path).await.unwrap();
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
            cache.read_value(&path).await.unwrap();
            cache.evict(&path);
            assert!(cache.is_empty());
            std::fs::write(&path, "46000\n").unwrap();
            let value = cache.read_value(&path).await.unwrap();
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
            let result = cache.read_value(&dir.path().join("absent_input")).await;
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
            assert_eq!(
                cache
                    .read_value(&path)
                    .await
                    .unwrap()
                    .trimmed_str()
                    .unwrap(),
                "45000"
            );
            std::fs::write(&replacement, "99000\n").unwrap();
            std::fs::rename(&replacement, &path).unwrap();
            assert_eq!(
                cache
                    .read_value(&path)
                    .await
                    .unwrap()
                    .trimmed_str()
                    .unwrap(),
                "45000",
                "a held descriptor must not be used for files that can be replaced"
            );
        });
    }
}
