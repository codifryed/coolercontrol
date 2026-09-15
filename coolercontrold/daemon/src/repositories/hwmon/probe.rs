// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Re-probing for the one-shot hwmon detection pass.
//!
//! Detection and polling have opposite failure semantics. Polling absorbs a failed read: the
//! per-channel cache keeps the last known good value and the failsafe overlay takes over after
//! `MISSING_STATUS_THRESHOLD` consecutive misses, and the channel is never removed. Detection gets
//! one read, and a channel it gives up on is gone for the session: `HwmonRepo::reinitialize_devices`
//! is unsupported and not even resume-from-sleep re-probes. So detection re-reads a failure that
//! may not be real, and polling does not.

use crate::cc_fs;
use crate::rt;
use anyhow::Result;
use log::debug;
use std::ops::Not;
use std::path::Path;
use std::time::Duration;

/// Passes over one attribute during detection before the channel is given up on.
const DETECT_PROBE_PASSES: u8 = 3;
const _: () = assert!(DETECT_PROBE_PASSES > 0);

/// Wait between detection passes. A USB HID driver that just timed out over its own bounded wait
/// needs more than an immediate re-issue to recover.
const DETECT_PROBE_DELAY: Duration = Duration::from_millis(150);

/// Re-reads an attribute that failed transiently, so one blip cannot cost the channel for the
/// whole session. Returns the value, or the last error once the passes are spent.
///
/// Gating on `cc_fs::is_transient` is what bounds startup. The ordinary "this attribute is not
/// readable" errnos (ENOENT, EOPNOTSUPP, ENODATA, EACCES) are not transient, so a board full of
/// unreadable attributes pays one read each and no delay.
pub async fn read_until_ok<T>(path: &Path, mut read: impl AsyncFnMut() -> Result<T>) -> Result<T> {
    let mut passes = DETECT_PROBE_PASSES;
    // Bounded by `passes`, which drops by one per failure and returns the error at zero.
    let err = loop {
        let err = match read().await {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        debug_assert!(passes > 0);
        passes -= 1;
        if passes == 0 || cc_fs::is_transient(&err).not() {
            break err;
        }
        debug!(
            "Transient failure reading {}, re-probing: {err}",
            path.display()
        );
        rt::sleep(DETECT_PROBE_DELAY).await;
    };
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::Error;

    fn transient() -> anyhow::Error {
        Error::from_raw_os_error(nix::libc::EINTR).into()
    }

    /// Goal: a transient failure must not cost the channel. A dropped channel is gone for the
    /// session, so the probe has to re-read. Method: fail once, then succeed, asserting both the
    /// verdict and that it actually re-read.
    #[test]
    fn a_transient_failure_is_re_probed() {
        cc_fs::test_runtime(async {
            let calls = Cell::new(0_u8);
            let readable = read_until_ok(
                Path::new("/sys/class/hwmon/hwmon4/temp1_input"),
                async || {
                    calls.set(calls.get() + 1);
                    if calls.get() == 1 {
                        Err(transient())
                    } else {
                        Ok(42_u8)
                    }
                },
            )
            .await
            .is_ok();
            assert!(readable);
            assert_eq!(calls.get(), 2, "the probe did not re-read");
        });
    }

    /// Goal: the re-probe must be bounded, or a device that is genuinely gone stalls startup.
    /// Method: fail every pass and assert the pass count is exactly the budget.
    #[test]
    fn re_probing_is_bounded_by_the_pass_budget() {
        cc_fs::test_runtime(async {
            let calls = Cell::new(0_u8);
            let readable = read_until_ok(
                Path::new("/sys/class/hwmon/hwmon4/temp1_input"),
                async || {
                    calls.set(calls.get() + 1);
                    Err::<u8, _>(transient())
                },
            )
            .await
            .is_ok();
            assert!(readable.not());
            assert_eq!(calls.get(), DETECT_PROBE_PASSES);
        });
    }

    /// Goal: this is what keeps startup bounded. An attribute that is simply not readable is the
    /// common case on a populated board and must cost one read and no delay, not
    /// `DETECT_PROBE_PASSES` reads spaced by `DETECT_PROBE_DELAY`. Method: fail with ENODATA and
    /// assert a single pass.
    #[test]
    fn a_non_transient_failure_is_not_re_probed() {
        cc_fs::test_runtime(async {
            let calls = Cell::new(0_u8);
            let readable = read_until_ok(
                Path::new("/sys/class/hwmon/hwmon4/temp1_input"),
                async || {
                    calls.set(calls.get() + 1);
                    Err::<u8, _>(Error::from_raw_os_error(nix::libc::ENODATA).into())
                },
            )
            .await
            .is_ok();
            assert!(readable.not());
            assert_eq!(calls.get(), 1, "an unreadable attribute was re-probed");
        });
    }
}
