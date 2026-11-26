// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Works around the `spd5118` driver leaving a DDR5 temperature sensor switched off.
//!
//! The driver disables the sensor on suspend and re-enables it on resume with a single write.
//! When the `SMBus` is busy that write fails, the sensor stays off and reads 0, and the driver
//! still reports it as enabled. The DIMM hub keeps standby power, so this survives a reboot.

use crate::repositories::hwmon::device_io::DeviceIo;
use crate::rt;
use anyhow::{anyhow, Result};
use log::{debug, warn};
use std::ops::Not;
use std::path::Path;
use std::time::Duration;

pub const DEVICE_NAME: &str = "spd5118";

const TEMP_ENABLE_FILE: &str = "temp1_enable";
const SENSOR_DISABLED: &[u8] = b"0";
const SENSOR_ENABLED: &[u8] = b"1";
/// The bus can still be busy shortly after resume, which is how the sensor got stuck.
const ATTEMPTS_MAX: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

const _: () = assert!(ATTEMPTS_MAX > 0);

/// Switches the temperature sensor of a `spd5118` device back on. Any other device is left
/// alone. A failure is logged and not returned: the sensor then reads 0, as it did before.
pub async fn reenable_sensor(device_name: &str, hwmon_path: &Path, io: &DeviceIo) {
    if device_name != DEVICE_NAME {
        return;
    }
    let enable_path = hwmon_path.join(TEMP_ENABLE_FILE);
    // Also keeps a kernel without the attribute from logging a failure.
    if io.read_value(&enable_path).await.is_err() {
        debug!(
            "No readable {TEMP_ENABLE_FILE} under {}, leaving its sensor as it is",
            hwmon_path.display()
        );
        return;
    }
    match toggle_with_retries(&enable_path, io, RETRY_DELAY).await {
        Ok(()) => debug!(
            "Re-enabled the temperature sensor of {}",
            hwmon_path.display()
        ),
        Err(err) => warn!(
            "Could not re-enable the DDR5 temperature sensor of {}, it may read 0: {err}",
            hwmon_path.display()
        ),
    }
}

/// Repeats the whole toggle until it succeeds, at most `ATTEMPTS_MAX` times.
///
/// Never the enabling write alone: after a failed write the driver's cached state can already
/// say enabled, and it drops a write that matches its cache.
async fn toggle_with_retries(
    enable_path: &Path,
    io: &DeviceIo,
    retry_delay: Duration,
) -> Result<()> {
    debug_assert!(enable_path.ends_with(TEMP_ENABLE_FILE));
    let mut last_error = anyhow!("no attempt was made");
    for attempt in 1..=ATTEMPTS_MAX {
        match toggle(enable_path, io).await {
            Ok(()) => return Ok(()),
            Err(err) => last_error = err,
        }
        if attempt < ATTEMPTS_MAX {
            rt::sleep(retry_delay).await;
        }
    }
    debug_assert!(last_error.to_string().is_empty().not());
    Err(last_error)
}

/// Off, then on. The driver believes the sensor is on and would drop a lone enabling write, so
/// the disabling write comes first to make the enabling one reach the hardware.
async fn toggle(enable_path: &Path, io: &DeviceIo) -> Result<()> {
    io.write_value(enable_path, SENSOR_DISABLED.to_vec())
        .await?;
    io.write_value(enable_path, SENSOR_ENABLED.to_vec()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::hwmon::device_io::Request;
    use std::path::PathBuf;

    const TEST_RETRY_DELAY: Duration = Duration::from_millis(1);

    /// Answers a device's writes on the test thread, failing the first `failures` of them, and
    /// returns every write it saw once `future` completes.
    async fn serve_writes<F: Future>(
        mut rx: tokio::sync::mpsc::Receiver<Request>,
        failures: usize,
        future: F,
    ) -> (F::Output, Vec<(PathBuf, String)>) {
        let mut writes = Vec::new();
        let output = {
            let serving = async {
                while let Some(request) = rx.recv().await {
                    if let Request::Write { path, data, reply } = request {
                        let answer = if writes.len() < failures {
                            Err(anyhow!("bus busy"))
                        } else {
                            Ok(())
                        };
                        writes.push((path, String::from_utf8_lossy(&data).into_owned()));
                        let _ = reply.send(answer);
                    }
                }
            };
            tokio::select! {
                output = future => output,
                () = serving => panic!("the device outlives the call"),
            }
        };
        (output, writes)
    }

    fn values(writes: &[(PathBuf, String)]) -> Vec<&str> {
        writes.iter().map(|(_, value)| value.as_str()).collect()
    }

    /// Goal: the sensor is switched off and then on, in that order, on its enable attribute.
    /// Method: a device whose writes are recorded.
    #[test]
    fn toggle_writes_off_then_on() {
        cc_fs::test_runtime(async {
            let (io, rx) = DeviceIo::wedged_for_test(Duration::from_secs(5));
            let enable_path = Path::new("/sys/fake/hwmon6").join(TEMP_ENABLE_FILE);

            let (result, writes) = serve_writes(
                rx,
                0,
                toggle_with_retries(&enable_path, &io, TEST_RETRY_DELAY),
            )
            .await;

            assert!(result.is_ok());
            assert_eq!(values(&writes), ["0", "1"]);
            assert!(writes.iter().all(|(path, _)| path == &enable_path));
        });
    }

    /// Goal: a failed write restarts the whole toggle, never the enabling write alone.
    /// Method: fail the first write and check the second attempt starts with the disabling one.
    #[test]
    fn a_failed_write_restarts_the_toggle() {
        cc_fs::test_runtime(async {
            let (io, rx) = DeviceIo::wedged_for_test(Duration::from_secs(5));
            let enable_path = Path::new("/sys/fake/hwmon6").join(TEMP_ENABLE_FILE);

            let (result, writes) = serve_writes(
                rx,
                1,
                toggle_with_retries(&enable_path, &io, TEST_RETRY_DELAY),
            )
            .await;

            assert!(result.is_ok());
            assert_eq!(values(&writes), ["0", "0", "1"]);
        });
    }

    /// Goal: the retries are bounded and the last failure is returned.
    /// Method: fail every write and count the attempts.
    #[test]
    fn gives_up_after_the_attempt_limit() {
        cc_fs::test_runtime(async {
            let (io, rx) = DeviceIo::wedged_for_test(Duration::from_secs(5));
            let enable_path = Path::new("/sys/fake/hwmon6").join(TEMP_ENABLE_FILE);

            let (result, writes) = serve_writes(
                rx,
                usize::MAX,
                toggle_with_retries(&enable_path, &io, TEST_RETRY_DELAY),
            )
            .await;

            assert!(result.is_err());
            // Each attempt stops at its failed disabling write.
            assert_eq!(writes.len(), usize::from(ATTEMPTS_MAX));
            assert!(values(&writes).iter().all(|value| *value == "0"));
        });
    }

    /// Goal: a `spd5118` device is re-enabled, and the file ends up enabled.
    /// Method: a real file that starts disabled.
    #[test]
    fn reenables_a_spd5118_device() {
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let enable_path = dir.path().join(TEMP_ENABLE_FILE);
            cc_fs::write(&enable_path, b"0".to_vec()).await.unwrap();

            reenable_sensor(DEVICE_NAME, dir.path(), &DeviceIo::default()).await;

            let enabled = cc_fs::read_sysfs(&enable_path).await.unwrap();
            assert_eq!(enabled.trim(), "1");
        });
    }

    /// Goal: no other driver's enable attribute is written: a user may have disabled it.
    /// Method: a disabled sensor under another device name stays disabled.
    #[test]
    fn leaves_other_devices_alone() {
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let enable_path = dir.path().join(TEMP_ENABLE_FILE);
            cc_fs::write(&enable_path, b"0".to_vec()).await.unwrap();

            reenable_sensor("jc42", dir.path(), &DeviceIo::default()).await;

            let enabled = cc_fs::read_sysfs(&enable_path).await.unwrap();
            assert_eq!(enabled.trim(), "0");
        });
    }

    /// Goal: a device without the attribute gets no write, so none is created.
    /// Method: an empty device directory.
    #[test]
    fn skips_a_device_without_the_attribute() {
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();

            reenable_sensor(DEVICE_NAME, dir.path(), &DeviceIo::default()).await;

            assert!(dir.path().join(TEMP_ENABLE_FILE).exists().not());
        });
    }
}
