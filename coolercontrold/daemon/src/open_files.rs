// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The process's open file limit. systemd's soft 1024 exists only for `select()` users, and
//! the daemon's descriptors grow with hardware: one per cached sysfs attribute and one per
//! API connection. So it raises a low soft limit toward the hard one, as systemd advises.

use log::{debug, warn};
use nix::sys::resource::{getrlimit, rlim_t, setrlimit, Resource};

/// Far more than the daemon can use: a few hundred sysfs attributes and at most a few hundred
/// API connections. A soft limit already this high is left alone, and a raise stops here.
const SUFFICIENT_OPEN_FILES: u64 = 65_536;
/// Below this, a system with a lot of hardware may run short of descriptors.
const COMFORTABLE_OPEN_FILES: u64 = 4096;
/// What to assume when the limit cannot be read: the traditional soft default.
const ASSUMED_OPEN_FILES: u64 = 1024;

const _: () = assert!(ASSUMED_OPEN_FILES < COMFORTABLE_OPEN_FILES);
const _: () = assert!(COMFORTABLE_OPEN_FILES < SUFFICIENT_OPEN_FILES);

/// Raises a low soft limit as far as the hard limit and `SUFFICIENT_OPEN_FILES` allow, and
/// warns if what remains is low.
/// Children inherit it, which only matters to one using `select()` past 1024 files.
pub fn raise_limit() {
    let Ok((soft, hard)) = getrlimit(Resource::RLIMIT_NOFILE) else {
        debug!("Could not read the open file limit; leaving it unchanged.");
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
/// Both outcomes log at DEBUG: when what remains is low, the caller WARNs with the remedy.
fn raise_soft_limit(soft: u64, hard: u64, apply: impl FnOnce(u64, u64) -> nix::Result<()>) -> u64 {
    let target = target_soft_limit(soft, hard);
    debug_assert!(target >= soft);
    if target == soft {
        return soft;
    }
    match apply(target, hard) {
        Ok(()) => {
            debug!("Raised the open file limit from {soft} to {target}.");
            target
        }
        Err(err) => {
            debug!("Could not raise the open file limit from {soft}: {err}");
            soft
        }
    }
}

/// The soft limit in force now.
pub fn limit() -> u64 {
    getrlimit(Resource::RLIMIT_NOFILE).map_or(ASSUMED_OPEN_FILES, |(soft, _)| widen(soft))
}

/// The soft limit to ask for: `SUFFICIENT_OPEN_FILES`, or the hard limit if that is lower.
/// Never lower than the current soft limit. An unlimited hard limit is `u64::MAX` here, so it
/// needs no special case, and the target stays far below the kernel's `fs.nr_open`.
fn target_soft_limit(soft: u64, hard: u64) -> u64 {
    soft.max(hard.min(SUFFICIENT_OPEN_FILES))
}

/// `rlim_t` is 32 bits wide on some targets, such as 32-bit glibc, and 64 on the rest.
#[allow(clippy::useless_conversion)] // Only useless where `rlim_t` is already `u64`.
fn widen(value: rlim_t) -> u64 {
    u64::from(value)
}

fn apply_soft_limit(soft: u64, hard: u64) -> nix::Result<()> {
    // Values came from `getrlimit` or are at most `SUFFICIENT_OPEN_FILES`, so both fit `rlim_t`.
    let soft = rlim_t::try_from(soft).map_err(|_| nix::Error::EINVAL)?;
    let hard = rlim_t::try_from(hard).map_err(|_| nix::Error::EINVAL)?;
    setrlimit(Resource::RLIMIT_NOFILE, soft, hard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::resource::RLIM_INFINITY;

    /// Goal: systemd's 1024 soft limit is raised to the sufficient level, not all the way to a
    /// large hard limit, and to the hard limit when that is lower.
    #[test]
    fn low_limit_is_raised_to_sufficient() {
        assert_eq!(target_soft_limit(1024, 524_288), SUFFICIENT_OPEN_FILES);
        assert_eq!(target_soft_limit(1024, 4096), 4096);
        assert_eq!(target_soft_limit(4096, 4096), 4096);
    }

    /// Goal: an unlimited hard limit needs no special case; the target is still sufficient.
    #[test]
    fn unlimited_hard_limit_targets_sufficient() {
        assert_eq!(
            target_soft_limit(1024, widen(RLIM_INFINITY)),
            SUFFICIENT_OPEN_FILES
        );
    }

    /// Goal: a soft limit already at or above sufficient is left alone, so 65536 is not pushed
    /// on to 524288, and the target never lowers a limit.
    #[test]
    fn sufficient_limit_is_left_alone() {
        assert_eq!(target_soft_limit(65_536, 524_288), 65_536);
        assert_eq!(target_soft_limit(524_288, 524_288), 524_288);
        assert_eq!(target_soft_limit(8192, 4096), 8192);
        let unchanged = raise_soft_limit(65_536, 524_288, |_, _| panic!("already sufficient"));
        assert_eq!(unchanged, 65_536);
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
        assert_eq!(effective, SUFFICIENT_OPEN_FILES);
        assert_eq!(applied, Some((SUFFICIENT_OPEN_FILES, 524_288)));
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
