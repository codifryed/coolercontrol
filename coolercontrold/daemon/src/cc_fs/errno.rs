// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errno classification shared by the sysfs read paths.
use nix::libc;

/// Errnos that mean the read did not happen and another try may succeed, never that the attribute
/// is unreadable.
///
/// `EINTR` is the one that matters: a driver that sleeps interruptibly inside its sysfs read
/// returns `-ERESTARTSYS`, which `io_uring` converts rather than restarting. `EIO` is deliberately
/// absent, being ambiguous and often permanent.
///
/// The hot read path retries only `EINTR`. This wider set is for one-shot detection, where giving
/// up costs a channel for the whole session.
pub fn is_transient(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>()
        .and_then(std::io::Error::raw_os_error)
        // EWOULDBLOCK is EAGAIN on Linux, so one arm covers both.
        .is_some_and(|errno| {
            matches!(
                errno,
                libc::EINTR | libc::ETIMEDOUT | libc::EAGAIN | libc::EBUSY
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    /// Goal: pin the positive space. Every errno that means "the read did not happen" must be
    /// retryable, or a transient blip costs a channel for the session. Method: classify one error
    /// per errno.
    #[test]
    fn retryable_errnos_are_transient() {
        for errno in [libc::EINTR, libc::ETIMEDOUT, libc::EAGAIN, libc::EBUSY] {
            let err: anyhow::Error = std::io::Error::from_raw_os_error(errno).into();
            assert!(is_transient(&err), "errno {errno} should be retried");
        }
    }

    /// Goal: pin the negative space, which is what bounds startup. Detection re-probes with a
    /// delay, so the ordinary "this attribute is not readable" errnos must cost nothing, and
    /// `get_pwm_duty` must keep treating them as a driver refusal. Method: classify one error per
    /// errno, plus a non-io error to cover the parse failures that share this path.
    #[test]
    fn unreadable_and_ambiguous_errnos_are_not_transient() {
        for errno in [
            libc::ENOENT,
            libc::EOPNOTSUPP,
            libc::ENODATA,
            libc::EACCES,
            libc::EIO,
            libc::ENODEV,
            libc::EPERM,
        ] {
            let err: anyhow::Error = std::io::Error::from_raw_os_error(errno).into();
            assert!(
                is_transient(&err).not(),
                "errno {errno} must not be retried"
            );
        }
        let parse_err: anyhow::Error = anyhow::anyhow!("invalid digit found in string");
        assert!(is_transient(&parse_err).not());
    }
}
