// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The process's open file limit. systemd's soft 1024 exists only for `select()` users, and
//! the daemon's descriptors grow with hardware: one per cached sysfs attribute and one per
//! API connection. So it raises the soft limit to the hard one, as systemd advises.

use log::{info, warn};
use nix::sys::resource::{getrlimit, rlim_t, setrlimit, Resource, RLIM_INFINITY};

/// The kernel refuses a soft limit above `fs.nr_open`, and this is its default. An unlimited
/// hard limit is clamped to it rather than making the raise fail.
const NR_OPEN_DEFAULT: u64 = 1 << 20;
/// Below this, a system with a lot of hardware may run short of descriptors.
const COMFORTABLE_OPEN_FILES: u64 = 4096;
/// What to assume when the limit cannot be read: the traditional soft default.
const ASSUMED_OPEN_FILES: u64 = 1024;

const _: () = assert!(ASSUMED_OPEN_FILES < COMFORTABLE_OPEN_FILES);
const _: () = assert!(COMFORTABLE_OPEN_FILES < NR_OPEN_DEFAULT);

/// Raises the soft limit as far as the hard limit allows, and warns if what remains is low.
/// Children inherit it, which only matters to one using `select()` past 1024 files.
pub fn raise_limit() {
    let Ok((soft, hard)) = getrlimit(Resource::RLIMIT_NOFILE) else {
        warn!("Could not read the open file limit; leaving it unchanged.");
        return;
    };
    let effective = raise_soft_limit(widen(soft), widen(hard), apply_soft_limit);
    if effective < COMFORTABLE_OPEN_FILES {
        warn!(
            "The open file limit is {effective}. A system with many devices may run out of file \
             descriptors. Raise the service's LimitNOFILE to at least {COMFORTABLE_OPEN_FILES}."
        );
    }
}

/// Raises `soft` toward `hard` through `apply`, and returns the soft limit left in force.
/// A failed raise is only INFO: when what remains is low, the caller WARNs with the remedy.
fn raise_soft_limit(soft: u64, hard: u64, apply: impl FnOnce(u64, u64) -> nix::Result<()>) -> u64 {
    let target = target_soft_limit(soft, hard);
    debug_assert!(target >= soft);
    if target == soft {
        return soft;
    }
    match apply(target, hard) {
        Ok(()) => {
            info!("Raised the open file limit from {soft} to {target}.");
            target
        }
        Err(err) => {
            info!("Could not raise the open file limit from {soft}: {err}");
            soft
        }
    }
}

/// The soft limit in force now.
pub fn limit() -> u64 {
    getrlimit(Resource::RLIMIT_NOFILE).map_or(ASSUMED_OPEN_FILES, |(soft, _)| widen(soft))
}

/// The soft limit to ask for: the hard limit, or the kernel's default ceiling when the hard
/// limit is unlimited. Never lower than the current soft limit.
fn target_soft_limit(soft: u64, hard: u64) -> u64 {
    let ceiling = if hard == widen(RLIM_INFINITY) {
        NR_OPEN_DEFAULT
    } else {
        hard
    };
    soft.max(ceiling)
}

/// `rlim_t` is 32 bits wide on some targets, such as 32-bit glibc, and 64 on the rest.
#[allow(clippy::useless_conversion)] // Only useless where `rlim_t` is already `u64`.
fn widen(value: rlim_t) -> u64 {
    u64::from(value)
}

fn apply_soft_limit(soft: u64, hard: u64) -> nix::Result<()> {
    // Values came from `getrlimit` or are at most `NR_OPEN_DEFAULT`, so both fit `rlim_t`.
    let soft = rlim_t::try_from(soft).map_err(|_| nix::Error::EINVAL)?;
    let hard = rlim_t::try_from(hard).map_err(|_| nix::Error::EINVAL)?;
    setrlimit(Resource::RLIMIT_NOFILE, soft, hard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a finite hard limit is the target, which is systemd's common 1024 / 524288 case.
    #[test]
    fn target_is_the_hard_limit() {
        assert_eq!(target_soft_limit(1024, 524_288), 524_288);
        assert_eq!(target_soft_limit(4096, 4096), 4096);
    }

    /// Goal: an unlimited hard limit is clamped to the kernel's default ceiling, since the
    /// kernel refuses an unlimited soft limit for open files.
    #[test]
    fn unlimited_hard_limit_is_clamped() {
        assert_eq!(
            target_soft_limit(1024, widen(RLIM_INFINITY)),
            NR_OPEN_DEFAULT
        );
    }

    /// Goal: the target never lowers a soft limit that is already higher.
    #[test]
    fn target_never_lowers_the_limit() {
        assert_eq!(target_soft_limit(8192, 4096), 8192);
        assert_eq!(
            target_soft_limit(NR_OPEN_DEFAULT * 2, widen(RLIM_INFINITY)),
            NR_OPEN_DEFAULT * 2
        );
    }

    /// Goal: a successful raise reports the target as the limit in force. Method: a stub
    /// apply records what it was asked to set.
    #[test]
    fn raise_reports_the_applied_target() {
        let mut applied = None;
        let effective = raise_soft_limit(1024, 524_288, |soft, hard| {
            applied = Some((soft, hard));
            Ok(())
        });
        assert_eq!(effective, 524_288);
        assert_eq!(applied, Some((524_288, 524_288)));
    }

    /// Goal: a refused raise keeps the soft limit, and a limit already at its target is
    /// never re-applied. Method: stubs that fail, or panic if called.
    #[test]
    fn raise_failure_keeps_the_soft_limit() {
        let effective = raise_soft_limit(1024, 524_288, |_, _| Err(nix::Error::EPERM));
        assert_eq!(effective, 1024);
        let unchanged = raise_soft_limit(4096, 4096, |_, _| panic!("nothing to raise"));
        assert_eq!(unchanged, 4096);
    }

    /// Goal: raising runs for real in this process and never lowers the limit. Method: read
    /// the limit around the call; whatever the environment allows, it must not go down.
    #[test]
    fn raise_never_lowers_the_process_limit() {
        let before = limit();
        raise_limit();
        let after = limit();
        assert!(after >= before);
        let (_, hard) = getrlimit(Resource::RLIMIT_NOFILE).unwrap();
        assert!(after <= widen(hard));
    }
}
