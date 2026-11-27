// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Works around the `spd5118` driver leaving a DDR5 temperature sensor switched off.
//!
//! The driver disables the sensor on suspend and re-enables it on resume with a single write.
//! When the `SMBus` is busy that write fails, the sensor stays off and reads 0, and the driver
//! still reports it as enabled. The DIMM hub keeps standby power, so this survives a reboot.
//!
//! Only a sensor that reads exactly 0 is toggled, as a switched-off one does. A toggle that
//! fails halfway would otherwise switch off a sensor that was working.

use crate::cc_fs::SysfsValue;
use crate::repositories::hwmon::device_io::{self, DeviceIo};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};
use crate::rt;
use anyhow::Result;
use log::{debug, info};
use std::ops::Not;
use std::path::Path;
use std::time::Duration;

pub const DEVICE_NAME: &str = "spd5118";

const TEMP_ENABLE_SUFFIX: &str = "_enable";
const SENSOR_DISABLED: &[u8] = b"0";
const SENSOR_ENABLED: &[u8] = b"1";
/// The bus can still be busy shortly after resume, which is how the sensor got stuck.
const ATTEMPTS_MAX: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

const _: () = assert!(ATTEMPTS_MAX > 0);

/// Whether a device's bus answered the last request sent to it.
///
/// The DIMMs share one bus, so a caller with more of them stops at the first that stalled
/// instead of waiting out a reply timeout for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorBus {
    Answered,
    Stalled,
}

impl SensorBus {
    fn after(err: &anyhow::Error) -> Self {
        if device_io::is_unanswered(err) {
            Self::Stalled
        } else {
            Self::Answered
        }
    }
}

/// Whether this module applies to a device, by the name its hwmon `name` attribute gives.
/// Never by its display name, which replaces that one after detection.
pub fn affects(hwmon_name: &str) -> bool {
    hwmon_name == DEVICE_NAME
}

/// Switches the temperature sensors of an affected device back on. A sensor that is not among
/// `channels` is left alone: the user disabled or ignores it.
///
/// Each temp channel is handled by its own hwmon number, and the pass ends at the first that
/// stalls.
pub async fn reenable_sensors(
    hwmon_path: &Path,
    channels: &[HwmonChannelInfo],
    io: &DeviceIo,
) -> SensorBus {
    for channel in channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        if reenable_sensor(hwmon_path, channel.number, io).await == SensorBus::Stalled {
            return SensorBus::Stalled;
        }
    }
    SensorBus::Answered
}

/// Switches the sensor behind `temp<temp_number>` back on when it reads exactly 0, as a
/// switched-off one does. A sensor with any other or no reading is left alone. A failure is
/// logged and not returned: the sensor then reads 0, as it did before.
async fn reenable_sensor(hwmon_path: &Path, temp_number: u8, io: &DeviceIo) -> SensorBus {
    let input_path = hwmon_path.join(temp_input_file(temp_number));
    let reading = match io.read_value(&input_path).await {
        Ok(reading) => reading,
        Err(err) => {
            debug!(
                "{} is not readable, leaving its sensor as it is: {err}",
                input_path.display()
            );
            return SensorBus::after(&err);
        }
    };
    if reads_zero(&reading).not() {
        return SensorBus::Answered;
    }
    let enable_path = hwmon_path.join(temp_enable_file(temp_number));
    match toggle_with_retries(&enable_path, io, RETRY_DELAY).await {
        Ok(()) => {
            debug!(
                "Re-enabled the temperature sensor at {}",
                input_path.display()
            );
            SensorBus::Answered
        }
        Err(err) => {
            // Recurs on every wake and names nothing the user can do, so not a warning.
            info!(
                "Could not re-enable the DDR5 temperature sensor at {}, it may read 0: {err}",
                input_path.display()
            );
            SensorBus::after(&err)
        }
    }
}

fn temp_input_file(temp_number: u8) -> String {
    format!("temp{temp_number}_input")
}

fn temp_enable_file(temp_number: u8) -> String {
    format!("temp{temp_number}{TEMP_ENABLE_SUFFIX}")
}

/// Whether a temp input reading, in millidegrees, is exactly 0. One that does not parse is
/// not: the toggle is only for the known symptom.
fn reads_zero(reading: &SysfsValue) -> bool {
    matches!(reading.parse::<i64>(), Ok(0))
}

/// Repeats the whole toggle until it succeeds, at most `ATTEMPTS_MAX` times, and only while
/// the device answers.
///
/// Never the enabling write alone: after a failed write the driver's cached state can already
/// say enabled, and it drops a write that matches its cache.
async fn toggle_with_retries(
    enable_path: &Path,
    io: &DeviceIo,
    retry_delay: Duration,
) -> Result<()> {
    debug_assert!(enable_path
        .to_str()
        .is_some_and(|path| path.ends_with(TEMP_ENABLE_SUFFIX)));
    for attempt in 1..=ATTEMPTS_MAX {
        let Err(err) = toggle(enable_path, io).await else {
            return Ok(());
        };
        // An unanswered write stays queued and lands once the bus recovers. Another attempt
        // would wait out the reply timeout again and queue one more disabling write.
        if device_io::is_unanswered(&err) {
            return Err(err);
        }
        if attempt == ATTEMPTS_MAX {
            return Err(err);
        }
        rt::sleep(retry_delay).await;
    }
    unreachable!("ATTEMPTS_MAX > 0, and the last attempt returns")
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
    use anyhow::anyhow;
    use std::path::PathBuf;

    const TEST_RETRY_DELAY: Duration = Duration::from_millis(1);
    /// Short, as the tests of a stalled device wait it out in real time.
    const TEST_REPLY_TIMEOUT: Duration = Duration::from_millis(20);

    /// Every request still queued for a device nobody served.
    fn queued(rx: &mut tokio::sync::mpsc::Receiver<Request>) -> Vec<Request> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

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

    fn temp_channel(number: u8) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number,
            name: format!("temp{number}"),
            ..Default::default()
        }
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
            let enable_path = Path::new("/sys/fake/hwmon6").join(temp_enable_file(1));

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
            let enable_path = Path::new("/sys/fake/hwmon6").join(temp_enable_file(1));

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
            let enable_path = Path::new("/sys/fake/hwmon6").join(temp_enable_file(1));

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

    /// Goal: a write that timed out is not retried: each attempt would wait out the full
    /// timeout and leave one more disabling write queued for when the bus recovers.
    /// Method: a device that never answers, then count what was queued for it.
    #[test]
    fn a_timed_out_write_is_not_retried() {
        cc_fs::test_runtime(async {
            let (io, mut rx) = DeviceIo::wedged_for_test(TEST_REPLY_TIMEOUT);
            let enable_path = Path::new("/sys/fake/hwmon6").join(temp_enable_file(1));

            let result = toggle_with_retries(&enable_path, &io, TEST_RETRY_DELAY).await;

            assert!(device_io::is_unanswered(&result.unwrap_err()));
            let requests = queued(&mut rx);
            assert_eq!(requests.len(), 1);
            let Request::Write { data, .. } = &requests[0] else {
                panic!("the one request is the disabling write");
            };
            assert_eq!(data.as_slice(), SENSOR_DISABLED);
        });
    }

    /// Goal: the caller learns of a stalled bus, and an answered error is not taken for one.
    /// Method: a device that never answers, then one whose every write fails.
    #[test]
    fn reports_a_stalled_bus_only_when_unanswered() {
        cc_fs::test_runtime(async {
            let (wedged_io, mut wedged_rx) = DeviceIo::wedged_for_test(TEST_REPLY_TIMEOUT);
            let hwmon_path = Path::new("/sys/fake/hwmon6");

            let stalled = reenable_sensors(hwmon_path, &[temp_channel(1)], &wedged_io).await;

            assert_eq!(stalled, SensorBus::Stalled);
            // The read of the sensor timed out, so no write followed it.
            let requests = queued(&mut wedged_rx);
            assert_eq!(requests.len(), 1);
            assert!(matches!(requests[0], Request::Read { .. }));

            let (io, rx) = DeviceIo::wedged_for_test(Duration::from_secs(5));
            let enable_path = hwmon_path.join(temp_enable_file(1));
            let (result, _writes) = serve_writes(
                rx,
                usize::MAX,
                toggle_with_retries(&enable_path, &io, TEST_RETRY_DELAY),
            )
            .await;
            assert_eq!(SensorBus::after(&result.unwrap_err()), SensorBus::Answered);
        });
    }

    /// A device directory with a disabled sensor, and `temp1_input` holding `reading` if any.
    /// Disabled, so that a toggle shows: it leaves the enable attribute at 1.
    async fn device_dir(reading: Option<&str>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let enable_path = dir.path().join(temp_enable_file(1));
        cc_fs::write(&enable_path, b"0".to_vec()).await.unwrap();
        if let Some(reading) = reading {
            cc_fs::write(
                dir.path().join(temp_input_file(1)),
                reading.as_bytes().to_vec(),
            )
            .await
            .unwrap();
        }
        (dir, enable_path)
    }

    async fn enable_value(enable_path: &Path) -> String {
        let enabled = cc_fs::read_sysfs(enable_path).await.unwrap();
        enabled.trim().to_owned()
    }

    /// Goal: a `spd5118` sensor that reads 0 is toggled, and ends up enabled.
    /// Method: real files, with the reading as the kernel gives it.
    #[test]
    fn toggles_a_sensor_that_reads_zero() {
        cc_fs::test_runtime(async {
            let (dir, enable_path) = device_dir(Some("0\n")).await;

            let bus = reenable_sensors(dir.path(), &[temp_channel(1)], &DeviceIo::default()).await;

            assert_eq!(bus, SensorBus::Answered);
            assert_eq!(enable_value(&enable_path).await, "1");
        });
    }

    /// Goal: a working sensor gets no write: a toggle that failed halfway would switch it off.
    /// Method: non-zero readings, then check the enable attribute was not written.
    #[test]
    fn leaves_a_sensor_with_a_reading_alone() {
        cc_fs::test_runtime(async {
            for reading in ["35000\n", "-250\n", "1\n"] {
                let (dir, enable_path) = device_dir(Some(reading)).await;

                let bus =
                    reenable_sensors(dir.path(), &[temp_channel(1)], &DeviceIo::default()).await;

                assert_eq!(bus, SensorBus::Answered);
                assert_eq!(enable_value(&enable_path).await, "0", "reading {reading:?}");
            }
        });
    }

    /// Goal: without a reading that is known to be 0 nothing is written.
    /// Method: a device with no `temp1_input`, then ones whose reading does not parse.
    #[test]
    fn leaves_a_sensor_without_a_usable_reading_alone() {
        cc_fs::test_runtime(async {
            for reading in [None, Some("\n"), Some("N/A\n"), Some("0.0\n")] {
                let (dir, enable_path) = device_dir(reading).await;

                let bus =
                    reenable_sensors(dir.path(), &[temp_channel(1)], &DeviceIo::default()).await;

                assert_eq!(bus, SensorBus::Answered);
                assert_eq!(enable_value(&enable_path).await, "0", "reading {reading:?}");
            }
        });
    }

    /// Goal: a sensor the user disabled or ignores is not toggled, and each remaining one is
    /// toggled through the attributes of its own hwmon number.
    /// Method: sensors 1 and 2 both off and reading 0. Pass no channel, then a fan numbered 1,
    /// then only temp channel 2.
    #[test]
    fn toggles_only_the_given_temp_channels_by_number() {
        cc_fs::test_runtime(async {
            let (dir, enable_1) = device_dir(Some("0\n")).await;
            let enable_2 = dir.path().join(temp_enable_file(2));
            cc_fs::write(&enable_2, b"0".to_vec()).await.unwrap();
            cc_fs::write(dir.path().join(temp_input_file(2)), b"0\n".to_vec())
                .await
                .unwrap();
            let io = DeviceIo::default();
            let fan = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                ..Default::default()
            };

            reenable_sensors(dir.path(), &[], &io).await;
            reenable_sensors(dir.path(), &[fan], &io).await;
            assert_eq!(enable_value(&enable_1).await, "0");
            assert_eq!(enable_value(&enable_2).await, "0");

            reenable_sensors(dir.path(), &[temp_channel(2)], &io).await;
            assert_eq!(enable_value(&enable_1).await, "0");
            assert_eq!(enable_value(&enable_2).await, "1");
        });
    }

    /// Goal: only the `spd5118` driver is affected: a user may have disabled another driver's
    /// sensor on purpose.
    /// Method: the driver name, another DIMM driver, and the display name a DIMM gets later.
    #[test]
    fn affects_only_the_spd5118_driver() {
        assert!(affects(DEVICE_NAME));
        assert!(affects("jc42").not());
        assert!(affects("DIMM 0x51").not());
        assert!(affects("").not());
    }
}
