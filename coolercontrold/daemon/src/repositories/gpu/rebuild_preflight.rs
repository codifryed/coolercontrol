// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks made before a boot image rebuild starts, while nothing is written yet. A package
//! manager may be rebuilding the same images, and a full `/boot` or EFI partition can leave a
//! truncated image behind.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::cc_fs;

/// Created by pacman for the length of a transaction, and left behind if it crashes.
const PACMAN_LOCK_PATH: &str = "var/lib/pacman/db.lck";
/// Held with `fcntl` locks while dpkg, apt or rpm (and so dnf and zypper) run.
const FCNTL_LOCK_PATHS: [&str; 4] = [
    "var/lib/dpkg/lock-frontend",
    "var/lib/dpkg/lock",
    "var/lib/rpm/.rpm.lock",
    "usr/lib/sysimage/rpm/.rpm.lock",
];
/// libzypp writes its pid here while it holds the lock.
const ZYPP_PID_PATH: &str = "run/zypp.pid";
/// Where boot images live: `/boot`, and the EFI partition under either common mount point.
const BOOT_IMAGE_DIRS: [&str; 3] = ["boot", "efi", "boot/efi"];
/// Deep enough for `EFI/Linux/*.efi` and `<machine-id>/<version>/initrd` under the EFI
/// partition, and `efi/EFI/Linux/*.efi` under `/boot`.
const SCAN_DEPTH_MAX: usize = 4;
const SCAN_DIR_ENTRIES_MAX: usize = 256;
const _: () = assert!(SCAN_DIR_ENTRIES_MAX > 0);
/// A tool writes each new image before the old one goes, and a new image can be larger.
const FREE_SPACE_IMAGE_MULTIPLE: u64 = 2;
const _: () = assert!(FREE_SPACE_IMAGE_MULTIPLE >= 1);
const BYTES_PER_MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightProblem {
    /// A package manager holds its lock. `stale_lock` names a lock file left by a crash.
    PackageManagerBusy {
        lock: String,
        stale_lock: Option<String>,
    },
    LowSpace {
        dir: PathBuf,
        free_bytes: u64,
        needed_bytes: u64,
    },
}

impl PreflightProblem {
    pub fn message(&self) -> String {
        match self {
            Self::PackageManagerBusy {
                lock,
                stale_lock: None,
            } => format!(
                "A package manager is running ({lock} is locked), and it may be rebuilding the \
                boot images too. Try again once it has finished."
            ),
            Self::PackageManagerBusy {
                lock,
                stale_lock: Some(path),
            } => format!(
                "A package manager is running ({lock} is locked), and it may be rebuilding the \
                boot images too. Try again once it has finished. If none is running, a crash \
                left {path} behind; remove it first."
            ),
            Self::LowSpace {
                dir,
                free_bytes,
                needed_bytes,
            } => format!(
                "{} has {} MiB free, but rebuilding the boot images needs about {} MiB. Free \
                some space there, for example by removing old kernels, then try again.",
                dir.display(),
                free_bytes / BYTES_PER_MIB,
                needed_bytes.div_ceil(BYTES_PER_MIB)
            ),
        }
    }
}

/// Checks under `root` (`/` outside tests) with blocking calls. Run it off the runtime thread.
pub fn check_blocking(root: &Path) -> Option<PreflightProblem> {
    assert!(root.is_absolute());
    package_manager_problem(root).or_else(|| space_problem_blocking(root))
}

fn package_manager_problem(root: &Path) -> Option<PreflightProblem> {
    let pacman_lock = root.join(PACMAN_LOCK_PATH);
    if cc_fs::exists(&pacman_lock) {
        return Some(PreflightProblem::PackageManagerBusy {
            lock: "pacman".to_owned(),
            stale_lock: Some(format!("/{PACMAN_LOCK_PATH}")),
        });
    }
    if let Some(path) = FCNTL_LOCK_PATHS
        .iter()
        .find(|path| fcntl_lock_held(&root.join(path)))
    {
        return Some(PreflightProblem::PackageManagerBusy {
            lock: format!("/{path}"),
            stale_lock: None,
        });
    }
    if zypp_lock_held(root) {
        return Some(PreflightProblem::PackageManagerBusy {
            lock: format!("/{ZYPP_PID_PATH}"),
            stale_lock: None,
        });
    }
    None
}

/// Asks whether another process holds a lock on the file. A missing file is not locked.
fn fcntl_lock_held(path: &Path) -> bool {
    use nix::fcntl::{fcntl, FcntlArg};
    let Ok(file) = cc_fs::open_options().read(true).open(path) else {
        return false;
    };
    let mut lock = whole_file_write_lock();
    if fcntl(&file, FcntlArg::F_GETLK(&mut lock)).is_err() {
        return false;
    }
    // F_GETLK leaves the type at F_UNLCK when nothing would conflict.
    i32::from(lock.l_type) != nix::libc::F_UNLCK
}

/// A write lock over the whole file: a zero start and length reach to its end.
// The lock constants are declared as `c_int` but are 0 to 2, so they fit `flock`'s `c_short`.
#[allow(clippy::cast_possible_truncation)]
fn whole_file_write_lock() -> nix::libc::flock {
    use nix::libc;
    // SAFETY: `flock` is a plain C struct, for which all zero bytes is a valid value.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    // Pair assertion: the lock answers for the whole file.
    assert_eq!(lock.l_start, 0);
    assert_eq!(lock.l_len, 0);
    lock
}

/// The file holds a pid only while the lock is held, and the process must still exist.
fn zypp_lock_held(root: &Path) -> bool {
    let Ok(bytes) = cc_fs::read_bytes_blocking(&root.join(ZYPP_PID_PATH)) else {
        return false;
    };
    let Ok(pid) = String::from_utf8_lossy(&bytes).trim().parse::<u32>() else {
        return false;
    };
    cc_fs::exists(root.join("proc").join(pid.to_string()))
}

fn space_problem_blocking(root: &Path) -> Option<PreflightProblem> {
    BOOT_IMAGE_DIRS.iter().find_map(|dir| {
        let dir = root.join(dir);
        let device = cc_fs::metadata(&dir).ok()?.dev();
        let largest_image_bytes = largest_image_bytes(&dir, device, 0);
        if largest_image_bytes == 0 {
            return None;
        }
        let stat = nix::sys::statvfs::statvfs(&dir).ok()?;
        // Both are narrower than u64 on 32-bit targets.
        #[allow(clippy::useless_conversion)]
        let free_bytes = u64::from(stat.blocks_available()) * u64::from(stat.fragment_size());
        space_problem(dir, free_bytes, largest_image_bytes)
    })
}

fn space_problem(
    dir: PathBuf,
    free_bytes: u64,
    largest_image_bytes: u64,
) -> Option<PreflightProblem> {
    let needed_bytes = largest_image_bytes.saturating_mul(FREE_SPACE_IMAGE_MULTIPLE);
    if free_bytes >= needed_bytes {
        return None;
    }
    Some(PreflightProblem::LowSpace {
        dir,
        free_bytes,
        needed_bytes,
    })
}

/// The largest boot image under `dir`, staying on the filesystem `device` so another
/// partition's images are not counted against this one's space.
fn largest_image_bytes(dir: &Path, device: u64, depth: usize) -> u64 {
    assert!(depth <= SCAN_DEPTH_MAX);
    let Ok(entries) = cc_fs::read_dir(dir) else {
        return 0;
    };
    let mut largest_bytes = 0;
    for entry in entries.take(SCAN_DIR_ENTRIES_MAX).filter_map(Result::ok) {
        // Not followed: a link could point anywhere, or loop.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.dev() != device {
            continue;
        }
        let path = entry.path();
        let size_bytes = if metadata.is_dir() {
            if depth == SCAN_DEPTH_MAX {
                continue;
            }
            largest_image_bytes(&path, device, depth + 1)
        } else if is_boot_image(&path) {
            if metadata.is_file() {
                metadata.len()
            } else {
                0
            }
        } else {
            0
        };
        largest_bytes = largest_bytes.max(size_bytes);
    }
    largest_bytes
}

/// initramfs and initrd files, and unified kernel images in `EFI/Linux`.
fn is_boot_image(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.starts_with("initramfs") || name.starts_with("initrd") {
        return true;
    }
    // The EFI partition is FAT, where names match in any case.
    let in_efi_linux = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|parent| parent.eq_ignore_ascii_case("linux"));
    let efi = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("efi"));
    in_efi_linux && efi
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    // Verify a pacman lock file counts as busy and names itself as possibly stale, and that a
    // clean root has no problem.
    #[test]
    fn a_pacman_lock_blocks_the_rebuild() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(package_manager_problem(root.path()), None);
        std::fs::create_dir_all(root.path().join("var/lib/pacman")).unwrap();
        std::fs::write(root.path().join(PACMAN_LOCK_PATH), "").unwrap();
        let problem = package_manager_problem(root.path()).unwrap();
        assert!(problem.message().contains("/var/lib/pacman/db.lck"));
    }

    // Verify the zypp pid file blocks only while its process exists, and an empty one, as
    // libzypp leaves it, does not.
    #[test]
    fn the_zypp_lock_needs_a_live_pid() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("run")).unwrap();
        std::fs::write(root.path().join(ZYPP_PID_PATH), "").unwrap();
        assert!(zypp_lock_held(root.path()).not());
        std::fs::write(root.path().join(ZYPP_PID_PATH), "4242\n").unwrap();
        assert!(zypp_lock_held(root.path()).not());
        std::fs::create_dir_all(root.path().join("proc/4242")).unwrap();
        assert!(zypp_lock_held(root.path()));
    }

    // Verify a lock file nobody holds, as dpkg leaves between runs, and a missing one, do not
    // count as busy.
    #[test]
    fn an_unheld_lock_file_is_not_busy() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lock");
        assert!(fcntl_lock_held(&path).not());
        std::fs::write(&path, "").unwrap();
        assert!(fcntl_lock_held(&path).not());
    }

    // Verify a write lock held through another open file is seen. An open file description
    // lock conflicts within one process, unlike a plain fcntl lock, which stands in for a
    // package manager here. Takes a real lock on the host filesystem, so it is gated.
    #[cfg(feature = "gated-tests")]
    #[test]
    fn a_held_lock_is_busy() {
        use nix::fcntl::{fcntl, FcntlArg};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lock");
        std::fs::write(&path, "").unwrap();
        let holder = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        fcntl(&holder, FcntlArg::F_OFD_SETLK(&whole_file_write_lock())).unwrap();
        assert!(fcntl_lock_held(&path));
    }

    // Verify the space check asks for twice the largest image, and passes at exactly that.
    #[test]
    fn space_for_twice_the_largest_image_is_enough() {
        let dir = PathBuf::from("/boot");
        assert_eq!(space_problem(dir.clone(), 200, 100), None);
        let problem = space_problem(dir, 199, 100).unwrap();
        assert_eq!(
            problem,
            PreflightProblem::LowSpace {
                dir: PathBuf::from("/boot"),
                free_bytes: 199,
                needed_bytes: 200,
            }
        );
    }

    // Verify the scan finds the largest initramfs, initrd or UKI at depth, and ignores kernels
    // and other files however large.
    #[test]
    fn the_largest_boot_image_is_found() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, size: usize| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, vec![0_u8; size]).unwrap();
        };
        write("vmlinuz-linux", 900);
        write("initramfs-linux.img", 300);
        write("machine-id/6.17.1/initrd", 400);
        write("EFI/Linux/linux-6.17.1.efi", 500);
        write("EFI/BOOT/BOOTX64.EFI", 800);
        let device = cc_fs::metadata(root.path()).unwrap().dev();
        assert_eq!(largest_image_bytes(root.path(), device, 0), 500);
        assert_eq!(
            largest_image_bytes(&root.path().join("missing"), device, 0),
            0
        );
    }
}
