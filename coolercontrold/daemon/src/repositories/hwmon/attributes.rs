// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Driver attributes read on demand for one hwmon channel: limits, targets and sensor details
//! that the per-tick pass never reads.

use std::ops::{Not, RangeInclusive};
use std::time::Duration;

use crate::cc_fs;
use crate::device::{ChannelAttribute, ChannelAttributeKind, MAX_CHANNEL_ATTRIBUTES, RPM};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::rt;
use anyhow::{anyhow, Result};
use log::debug;
use tokio::sync::{Semaphore, SemaphorePermit};

/// Longest an attribute read waits for a device permit. The read runs on the device API actor,
/// which serves no other request while it waits.
const ATTRIBUTE_PERMIT_TIMEOUT_MAX: Duration = Duration::from_secs(2);

/// Waits for the device permit, no longer than the device's read timeout or
/// `ATTRIBUTE_PERMIT_TIMEOUT_MAX`. `device_label` names the device and channel in the timeout
/// error.
pub async fn acquire_permit<'a>(
    semaphore: &'a Semaphore,
    read_permit_timeout: Duration,
    device_label: &str,
) -> Result<SemaphorePermit<'a>> {
    let permit_timeout = read_permit_timeout.min(ATTRIBUTE_PERMIT_TIMEOUT_MAX);
    debug_assert!(permit_timeout > Duration::ZERO);
    debug_assert!(permit_timeout <= ATTRIBUTE_PERMIT_TIMEOUT_MAX);
    tokio::select! {
        () = rt::sleep(permit_timeout) => {
            Err(anyhow!("TIMEOUT {device_label}; waiting to read attributes"))
        }
        permit = semaphore.acquire() => permit.map_err(|err| anyhow!(err)),
    }
}

struct AttributeSpec {
    suffix: &'static str,
    kind: ChannelAttributeKind,
}

const fn spec(suffix: &'static str, kind: ChannelAttributeKind) -> AttributeSpec {
    AttributeSpec { suffix, kind }
}

/// Display order: upper limits, lower limits, recorded extremes, the rated range, then sensor
/// details. Each hysteresis follows its limit.
const TEMP_ATTRIBUTES: [AttributeSpec; 16] = [
    spec("max", ChannelAttributeKind::TempMax),
    spec("max_hyst", ChannelAttributeKind::TempMaxHyst),
    spec("crit", ChannelAttributeKind::TempCrit),
    spec("crit_hyst", ChannelAttributeKind::TempCritHyst),
    spec("emergency", ChannelAttributeKind::TempEmergency),
    spec("emergency_hyst", ChannelAttributeKind::TempEmergencyHyst),
    spec("min", ChannelAttributeKind::TempMin),
    spec("min_hyst", ChannelAttributeKind::TempMinHyst),
    spec("lcrit", ChannelAttributeKind::TempLcrit),
    spec("lcrit_hyst", ChannelAttributeKind::TempLcritHyst),
    spec("lowest", ChannelAttributeKind::TempLowest),
    spec("highest", ChannelAttributeKind::TempHighest),
    spec("rated_min", ChannelAttributeKind::TempRatedMin),
    spec("rated_max", ChannelAttributeKind::TempRatedMax),
    spec("offset", ChannelAttributeKind::TempOffset),
    spec("type", ChannelAttributeKind::TempType),
];

const FAN_ATTRIBUTES: [AttributeSpec; 5] = [
    spec("min", ChannelAttributeKind::FanMin),
    spec("max", ChannelAttributeKind::FanMax),
    spec("target", ChannelAttributeKind::FanTarget),
    spec("div", ChannelAttributeKind::FanDiv),
    spec("pulses", ChannelAttributeKind::FanPulses),
];

/// Display order: upper limits, lower limits, the cap with its margin and range, then the
/// rated range.
const POWER_ATTRIBUTES: [AttributeSpec; 10] = [
    spec("max", ChannelAttributeKind::PowerMax),
    spec("crit", ChannelAttributeKind::PowerCrit),
    spec("min", ChannelAttributeKind::PowerMin),
    spec("lcrit", ChannelAttributeKind::PowerLcrit),
    spec("cap", ChannelAttributeKind::PowerCap),
    spec("cap_hyst", ChannelAttributeKind::PowerCapHyst),
    spec("cap_max", ChannelAttributeKind::PowerCapMax),
    spec("cap_min", ChannelAttributeKind::PowerCapMin),
    spec("rated_min", ChannelAttributeKind::PowerRatedMin),
    spec("rated_max", ChannelAttributeKind::PowerRatedMax),
];

const _: () = assert!(TEMP_ATTRIBUTES.len() <= MAX_CHANNEL_ATTRIBUTES);
const _: () = assert!(FAN_ATTRIBUTES.len() <= MAX_CHANNEL_ATTRIBUTES);
const _: () = assert!(POWER_ATTRIBUTES.len() <= MAX_CHANNEL_ATTRIBUTES);

/// Absolute zero. Drivers report it, or colder, for a limit that is not set.
const MILLIDEGREES_ABSOLUTE_ZERO: i64 = -273_150;
/// Hotter than any real limit. nvme reports 65261850 (`u16::MAX` Kelvin) for an unset one.
const MILLIDEGREES_MAX: i64 = 500_000;
const MILLIDEGREES_OFFSET_MAGNITUDE_MAX: i64 = 200_000;
const MILLIDEGREES_PER_DEGREE: f64 = 1000.0;
/// Faster than any real fan. Rejects the `0xFFFF`-style placeholders some chips report.
const FAN_RPM_MAX: i64 = 40_000;
/// A fan input whose driver label names another unit, such as the Leakshield's pressure in
/// µbar, has no range that tells a placeholder from a real limit. Its limits pass up to what
/// a reading can hold.
const FAN_NON_RPM_UNIT_MAX: i64 = RPM::MAX as i64;
const FAN_DIVISOR_MAX: i64 = 128;
const FAN_PULSES_MAX: i64 = 4;
const TEMP_TYPE_MAX: i64 = 6;
/// 100 kW, more than any supply in one machine delivers.
const MICROWATTS_MAX: i64 = 100_000_000_000;
const MICROWATTS_PER_WATT: f64 = 1_000_000.0;

/// Every integer up to here converts to f64 exactly, and so must every accepted value.
const F64_EXACT_INTEGER_MAX: i64 = 1 << 53;
const _: () = assert!(MILLIDEGREES_MAX <= F64_EXACT_INTEGER_MAX);
const _: () = assert!(MILLIDEGREES_ABSOLUTE_ZERO >= -F64_EXACT_INTEGER_MAX);
const _: () = assert!(FAN_RPM_MAX <= F64_EXACT_INTEGER_MAX);
const _: () = assert!(FAN_NON_RPM_UNIT_MAX <= F64_EXACT_INTEGER_MAX);
const _: () = assert!(FAN_RPM_MAX < FAN_NON_RPM_UNIT_MAX);
const _: () = assert!(MICROWATTS_MAX <= F64_EXACT_INTEGER_MAX);

/// Reads every attribute file the channel has. Absent files are skipped without driver IO;
/// unreadable, unparseable and unset values are left out.
///
/// The caller owns device serialization (the hwmon permit). This makes at most one read per
/// table entry, and only for files that exist.
pub async fn read_channel_attributes(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
) -> Vec<ChannelAttribute> {
    let (prefix, specs): (&str, &[AttributeSpec]) = match channel.hwmon_type {
        HwmonChannelType::Temp => ("temp", &TEMP_ATTRIBUTES),
        HwmonChannelType::Fan => ("fan", &FAN_ATTRIBUTES),
        HwmonChannelType::Power => ("power", &POWER_ATTRIBUTES),
        HwmonChannelType::Load | HwmonChannelType::Freq | HwmonChannelType::PowerCap => {
            return Vec::new()
        }
    };
    debug_assert!(specs.len() <= MAX_CHANNEL_ATTRIBUTES);
    let fan_limit_max = fan_limit_max(channel);
    let mut attributes = Vec::with_capacity(specs.len());
    for spec in specs {
        let name = format!("{prefix}{}_{}", channel.number, spec.suffix);
        let Some(raw) = read_raw(driver, &name).await else {
            continue;
        };
        let Some(value) = sanitize_attribute(spec.kind, raw, fan_limit_max) else {
            debug!(
                "Skipping unset hwmon attribute {name}={raw} on {}",
                driver.name
            );
            continue;
        };
        attributes.push(ChannelAttribute {
            name,
            kind: spec.kind,
            value,
        });
    }
    debug_assert!(attributes.len() <= specs.len());
    attributes
}

/// The highest fan min, max or target the channel can really report.
fn fan_limit_max(channel: &HwmonChannelInfo) -> i64 {
    if channel.caps.has_non_rpm_unit() {
        FAN_NON_RPM_UNIT_MAX
    } else {
        FAN_RPM_MAX
    }
}

/// Converts a raw attribute to its unit, or `None` for a placeholder that means "not set".
///
/// The bounds are wide on purpose: they only reject values no real sensor reports.
pub fn sanitize_attribute(kind: ChannelAttributeKind, raw: i64, fan_limit_max: i64) -> Option<f64> {
    use ChannelAttributeKind as K;
    debug_assert!(fan_limit_max <= F64_EXACT_INTEGER_MAX);
    let (accepted, divisor): (RangeInclusive<i64>, f64) = match kind {
        K::TempMax
        | K::TempMaxHyst
        | K::TempCrit
        | K::TempCritHyst
        | K::TempEmergency
        | K::TempEmergencyHyst
        | K::TempMin
        | K::TempMinHyst
        | K::TempLcrit
        | K::TempLcritHyst
        | K::TempLowest
        | K::TempHighest
        | K::TempRatedMin
        | K::TempRatedMax => (
            (MILLIDEGREES_ABSOLUTE_ZERO + 1)..=MILLIDEGREES_MAX,
            MILLIDEGREES_PER_DEGREE,
        ),
        K::TempOffset => (
            -MILLIDEGREES_OFFSET_MAGNITUDE_MAX..=MILLIDEGREES_OFFSET_MAGNITUDE_MAX,
            MILLIDEGREES_PER_DEGREE,
        ),
        K::TempType => (1..=TEMP_TYPE_MAX, 1.0),
        K::FanMin | K::FanMax | K::FanTarget => (0..=fan_limit_max, 1.0),
        K::FanDiv => (1..=FAN_DIVISOR_MAX, 1.0),
        K::FanPulses => (1..=FAN_PULSES_MAX, 1.0),
        K::PowerMax
        | K::PowerCrit
        | K::PowerMin
        | K::PowerLcrit
        | K::PowerCap
        | K::PowerCapHyst
        | K::PowerCapMax
        | K::PowerCapMin
        | K::PowerRatedMin
        | K::PowerRatedMax => (0..=MICROWATTS_MAX, MICROWATTS_PER_WATT),
    };
    if accepted.contains(&raw).not() {
        return None;
    }
    // Exact: every accepted range is inside f64's exact integers (asserted above).
    #[allow(clippy::cast_precision_loss)]
    let value = raw as f64 / divisor;
    debug_assert!(value.is_finite());
    Some(value)
}

/// `None` when the file is absent or its value cannot be read or parsed. Failures are logged at
/// debug only: the values are informational, and some drivers expose files that always error
/// on chips without the feature.
async fn read_raw(driver: &HwmonDriverInfo, name: &str) -> Option<i64> {
    let path = driver.path.join(name);
    // sysfs answers a stat itself, so an absent attribute costs no driver IO.
    if cc_fs::exists(&path).not() {
        return None;
    }
    let result = driver
        .io
        .read_value(&path)
        .await
        .and_then(|value| value.parse::<i64>());
    match result {
        Ok(raw) => Some(raw),
        Err(err) => {
            debug!("Could not read hwmon attribute {}: {err}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::hwmon::device_io::DeviceIo;
    use crate::repositories::hwmon::hwmon_repo::HwmonChannelCapabilities;
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    async fn test_dir() -> PathBuf {
        let path = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
        cc_fs::create_dir_all(&path).await.unwrap();
        path
    }

    async fn write_files(base: &Path, files: &[(&str, &str)]) {
        for (name, contents) in files {
            cc_fs::write(base.join(name), contents.as_bytes().to_vec())
                .await
                .unwrap();
        }
    }

    fn driver_at(path: &Path) -> HwmonDriverInfo {
        HwmonDriverInfo {
            name: "test".to_string(),
            path: path.to_path_buf(),
            io: DeviceIo::default(),
            ..Default::default()
        }
    }

    fn channel(hwmon_type: HwmonChannelType, number: u8) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type,
            number,
            ..Default::default()
        }
    }

    /// A fan input whose driver label names a unit other than rpm, e.g. `Pressure [ubar]`.
    fn unit_labelled_fan(number: u8) -> HwmonChannelInfo {
        HwmonChannelInfo {
            caps: HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::NON_RPM_UNIT,
            ..channel(HwmonChannelType::Fan, number)
        }
    }

    fn names(attributes: &[ChannelAttribute]) -> Vec<&str> {
        attributes.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn temp_limits_convert_and_reject_placeholders() {
        // Goal: real limits convert from millidegrees; the unset markers seen on nvme and amdgpu
        // are rejected. Method: boundaries on both sides of the accepted range.
        let k = ChannelAttributeKind::TempCrit;
        assert_eq!(sanitize_attribute(k, 94_850, FAN_RPM_MAX), Some(94.85));
        assert_eq!(sanitize_attribute(k, -5_150, FAN_RPM_MAX), Some(-5.15));
        assert_eq!(sanitize_attribute(k, 500_000, FAN_RPM_MAX), Some(500.0));
        assert_eq!(sanitize_attribute(k, -273_149, FAN_RPM_MAX), Some(-273.149));
        assert_eq!(sanitize_attribute(k, -273_150, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(k, -300_000, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(k, 500_001, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(k, 65_261_850, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(k, i64::MIN, FAN_RPM_MAX), None);
    }

    #[test]
    fn hysteresis_is_an_absolute_temperature() {
        // Goal: hysteresis values use the absolute-temperature bounds, so the amdgpu
        // `crit_hyst=-273150` marker is dropped. Method: the marker and one real value.
        let k = ChannelAttributeKind::TempCritHyst;
        assert_eq!(sanitize_attribute(k, -273_150, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(k, 105_000, FAN_RPM_MAX), Some(105.0));
    }

    #[test]
    fn offset_type_and_fan_values_use_their_own_bounds() {
        // Goal: each non-limit kind accepts its real range and nothing else. Method: the
        // accepted edges and the first rejected value past each.
        use ChannelAttributeKind as K;
        assert_eq!(sanitize_attribute(K::TempOffset, 0, FAN_RPM_MAX), Some(0.0));
        assert_eq!(
            sanitize_attribute(K::TempOffset, -200_000, FAN_RPM_MAX),
            Some(-200.0)
        );
        assert_eq!(
            sanitize_attribute(K::TempOffset, 200_001, FAN_RPM_MAX),
            None
        );
        assert_eq!(sanitize_attribute(K::TempType, 1, FAN_RPM_MAX), Some(1.0));
        assert_eq!(sanitize_attribute(K::TempType, 6, FAN_RPM_MAX), Some(6.0));
        assert_eq!(sanitize_attribute(K::TempType, 0, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(K::TempType, 7, FAN_RPM_MAX), None);
        // amdgpu reports fan1_min=0 as a real minimum.
        assert_eq!(sanitize_attribute(K::FanMin, 0, FAN_RPM_MAX), Some(0.0));
        assert_eq!(
            sanitize_attribute(K::FanMax, 40_000, FAN_RPM_MAX),
            Some(40_000.0)
        );
        assert_eq!(sanitize_attribute(K::FanMax, 65_535, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(K::FanTarget, -1, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(K::FanDiv, 0, FAN_RPM_MAX), None);
        assert_eq!(sanitize_attribute(K::FanDiv, 128, FAN_RPM_MAX), Some(128.0));
        assert_eq!(sanitize_attribute(K::FanPulses, 2, FAN_RPM_MAX), Some(2.0));
        assert_eq!(sanitize_attribute(K::FanPulses, 5, FAN_RPM_MAX), None);
    }

    #[test]
    fn rated_temps_are_absolute_temperatures() {
        // Goal: the rated range converts like any other limit and drops the unset markers.
        // Method: a real pair, then both placeholders.
        use ChannelAttributeKind as K;
        assert_eq!(
            sanitize_attribute(K::TempRatedMin, -40_000, FAN_RPM_MAX),
            Some(-40.0)
        );
        assert_eq!(
            sanitize_attribute(K::TempRatedMax, 125_000, FAN_RPM_MAX),
            Some(125.0)
        );
        assert_eq!(
            sanitize_attribute(K::TempRatedMin, -273_150, FAN_RPM_MAX),
            None
        );
        assert_eq!(
            sanitize_attribute(K::TempRatedMax, 65_261_850, FAN_RPM_MAX),
            None
        );
    }

    #[test]
    fn power_limits_convert_from_microwatts() {
        // Goal: power limits convert to watts across the whole accepted range, including
        // values past i32, and nothing outside it passes. Method: the amdgpu cap from real
        // hardware, a 3 kW supply, and both edges with the first rejected value past each.
        use ChannelAttributeKind as K;
        assert_eq!(
            sanitize_attribute(K::PowerCap, 230_000_000, FAN_RPM_MAX),
            Some(230.0)
        );
        assert_eq!(
            sanitize_attribute(K::PowerCapMin, 216_000_000, FAN_RPM_MAX),
            Some(216.0)
        );
        assert_eq!(
            sanitize_attribute(K::PowerCapHyst, 500_000, FAN_RPM_MAX),
            Some(0.5)
        );
        assert_eq!(
            sanitize_attribute(K::PowerRatedMax, 3_000_000_000, FAN_RPM_MAX),
            Some(3000.0)
        );
        assert_eq!(sanitize_attribute(K::PowerMin, 0, FAN_RPM_MAX), Some(0.0));
        assert_eq!(sanitize_attribute(K::PowerLcrit, -1, FAN_RPM_MAX), None);
        assert_eq!(
            sanitize_attribute(K::PowerMax, 100_000_000_000, FAN_RPM_MAX),
            Some(100_000.0)
        );
        assert_eq!(
            sanitize_attribute(K::PowerCrit, 100_000_000_001, FAN_RPM_MAX),
            None
        );
        assert_eq!(
            sanitize_attribute(K::PowerCapMax, i64::MAX, FAN_RPM_MAX),
            None
        );
    }

    #[test]
    fn unit_labelled_fan_limits_use_the_reading_range() {
        // Goal: a fan input whose driver label names another unit accepts limits up to what a
        // reading can hold, and every other fan keeps the rpm bound. Method: the Leakshield
        // pressures from issue #613 and the edges of each range.
        use ChannelAttributeKind as K;
        let unit_max = fan_limit_max(&unit_labelled_fan(1));
        assert_eq!(unit_max, i64::from(u32::MAX));
        assert_eq!(
            fan_limit_max(&channel(HwmonChannelType::Fan, 1)),
            FAN_RPM_MAX
        );
        assert_eq!(
            sanitize_attribute(K::FanMin, 382_800, unit_max),
            Some(382_800.0)
        );
        assert_eq!(
            sanitize_attribute(K::FanMax, 472_800, unit_max),
            Some(472_800.0)
        );
        assert_eq!(
            sanitize_attribute(K::FanMax, i64::from(u32::MAX), unit_max),
            Some(4_294_967_295.0)
        );
        assert_eq!(
            sanitize_attribute(K::FanMax, i64::from(u32::MAX) + 1, unit_max),
            None
        );
        assert_eq!(sanitize_attribute(K::FanMin, -1, unit_max), None);
        assert_eq!(sanitize_attribute(K::FanMax, 472_800, FAN_RPM_MAX), None);
    }

    #[test]
    #[serial]
    fn nvme_like_temp_keeps_only_set_limits() {
        // Goal: a channel reports only the limits that are really set, in display order.
        // Method: the nvme layout from real hardware, where temp2 has only placeholders.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("temp1_input", "40850"),
                    ("temp1_crit", "94850"),
                    ("temp1_max", "89850"),
                    ("temp1_min", "-273150"),
                    ("temp2_input", "41850"),
                    ("temp2_min", "-273150"),
                    ("temp2_max", "65261850"),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let temp1 = read_channel_attributes(&driver, &channel(HwmonChannelType::Temp, 1)).await;
            let temp2 = read_channel_attributes(&driver, &channel(HwmonChannelType::Temp, 2)).await;

            assert_eq!(names(&temp1), ["temp1_max", "temp1_crit"]);
            assert_eq!(temp1[0].kind, ChannelAttributeKind::TempMax);
            assert_eq!(temp1[0].value, 89.85);
            assert_eq!(temp1[1].value, 94.85);
            assert!(temp2.is_empty(), "placeholders only: {temp2:?}");
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn amdgpu_like_fan_keeps_zero_min_and_drops_zero_divisor() {
        // Goal: fan attributes read in table order; a zero minimum is real, a zero divisor is
        // not. Method: an amdgpu-style fan with one invalid divisor added.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("fan1_input", "937"),
                    ("fan1_min", "0"),
                    ("fan1_max", "3500"),
                    ("fan1_target", "1200"),
                    ("fan1_div", "0"),
                    ("fan1_pulses", "2"),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let fan1 = read_channel_attributes(&driver, &channel(HwmonChannelType::Fan, 1)).await;

            assert_eq!(
                names(&fan1),
                ["fan1_min", "fan1_max", "fan1_target", "fan1_pulses"]
            );
            assert_eq!(fan1[0].value, 0.0);
            assert_eq!(fan1[2].kind, ChannelAttributeKind::FanTarget);
            assert_eq!(fan1[2].value, 1200.0);
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn unit_labelled_fan_limits_are_kept() {
        // Goal: the channel's unit capability, not the driver name, selects the wide range
        // when reading files. Method: the Leakshield layout from issue #613 on a driver with
        // another name, read once with the capability and once without.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("fan1_input", "438300"),
                    ("fan1_min", "382800"),
                    ("fan1_max", "472800"),
                    ("fan1_target", "400000"),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let labelled = read_channel_attributes(&driver, &unit_labelled_fan(1)).await;
            let plain = read_channel_attributes(&driver, &channel(HwmonChannelType::Fan, 1)).await;

            assert_eq!(names(&labelled), ["fan1_min", "fan1_max", "fan1_target"]);
            assert_eq!(labelled[1].value, 472_800.0);
            assert!(plain.is_empty(), "rpm bound applies: {plain:?}");
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn amdgpu_like_power_reads_the_cap_and_its_range() {
        // Goal: a power channel reads its own attribute files, in table order, and nothing
        // the table does not name. Method: the amdgpu layout from real hardware, whose
        // `cap_default` is not in the hwmon ABI, next to another channel's files.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("power1_average", "36000000"),
                    ("power1_cap", "230000000"),
                    ("power1_cap_default", "230000000"),
                    ("power1_cap_max", "230000000"),
                    ("power1_cap_min", "216000000"),
                    ("power2_max", "90000000"),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let power1 =
                read_channel_attributes(&driver, &channel(HwmonChannelType::Power, 1)).await;

            assert_eq!(
                names(&power1),
                ["power1_cap", "power1_cap_max", "power1_cap_min"]
            );
            assert_eq!(power1[0].kind, ChannelAttributeKind::PowerCap);
            assert_eq!(power1[0].value, 230.0);
            assert_eq!(power1[2].value, 216.0);
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn rated_range_follows_the_recorded_extremes() {
        // Goal: a temp channel reports its rated range after the limits and extremes and
        // before the sensor details. Method: a PMBus-style temp with one of each group.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("temp1_input", "41000"),
                    ("temp1_type", "3"),
                    ("temp1_rated_max", "125000"),
                    ("temp1_rated_min", "-40000"),
                    ("temp1_highest", "58000"),
                    ("temp1_crit", "110000"),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let temp1 = read_channel_attributes(&driver, &channel(HwmonChannelType::Temp, 1)).await;

            assert_eq!(
                names(&temp1),
                [
                    "temp1_crit",
                    "temp1_highest",
                    "temp1_rated_min",
                    "temp1_rated_max",
                    "temp1_type"
                ]
            );
            assert_eq!(temp1[2].kind, ChannelAttributeKind::TempRatedMin);
            assert_eq!(temp1[2].value, -40.0);
            assert_eq!(temp1[3].value, 125.0);
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn unparseable_values_are_left_out() {
        // Goal: a garbage or empty file does not fail the whole read, and is not reported.
        // Method: one good attribute next to two bad ones.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(
                &base,
                &[
                    ("temp1_max", "80000\n"),
                    ("temp1_crit", "garbage"),
                    ("temp1_emergency", ""),
                ],
            )
            .await;
            let driver = driver_at(&base);

            let temp1 = read_channel_attributes(&driver, &channel(HwmonChannelType::Temp, 1)).await;

            assert_eq!(names(&temp1), ["temp1_max"]);
            assert_eq!(temp1[0].value, 80.0);
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }

    #[test]
    #[serial]
    fn other_channel_types_and_missing_dirs_report_nothing() {
        // Goal: only temp, fan and power channels have attribute tables, and a vanished device
        // dir yields an empty list rather than an error. Method: a load channel whose number
        // collides with real temp files, then a path that does not exist.
        cc_fs::test_runtime(async {
            let base = test_dir().await;
            write_files(&base, &[("temp1_max", "80000")]).await;
            let driver = driver_at(&base);
            let missing = driver_at(&base.join("gone"));

            let load = read_channel_attributes(&driver, &channel(HwmonChannelType::Load, 1)).await;
            let gone = read_channel_attributes(&missing, &channel(HwmonChannelType::Temp, 1)).await;

            assert!(load.is_empty());
            assert!(gone.is_empty());
            cc_fs::remove_dir_all(&base).await.unwrap();
        });
    }
}
