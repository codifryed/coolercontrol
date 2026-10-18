// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Golden tests for every status output: REST (`all`, `since`, recent, per device and per
//! channel), the SSE `status` frame and gRPC `Status`. `metrics.rs` pins the Prometheus text over
//! the same fixture. They pin the bytes clients receive, so a change to how history is stored
//! cannot change what is sent.
//!
//! Contract: values, presence, field omission and timestamp strings are exact. Device order is
//! not part of it (`AllDevices` is a `HashMap`), so devices are matched by UID. Within an entry,
//! temps and channels come out in each device's first-seen order. The fixture pushes every name in
//! that order, so entry order is compared exactly too.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[cfg(feature = "gated-tests")]
use axum::response::sse::{Event, Sse};
#[cfg(feature = "gated-tests")]
use axum::response::IntoResponse;
#[cfg(feature = "gated-tests")]
use chrono::TimeDelta;
use chrono::{DateTime, Local, Utc};
use tokio_util::sync::CancellationToken;

use crate::api::actor::StatusHandle;
use crate::api::devices::DeviceDto;
#[cfg(feature = "gated-tests")]
use crate::api::sse::SseEvent;
use crate::api::status::DeviceStatusDto;
#[cfg(feature = "gated-tests")]
use crate::api::status::{DeviceChannelStatusDto, StatusResponse};
use crate::device::{ChannelStatus, Device, DeviceInfo, DeviceType, Status, TempStatus, UID};
use crate::grpc_api::models;
use crate::grpc_api::models::v1::status::{FanSpeed, Metric};
use crate::AllDevices;

pub const POLL_RATE: f64 = 5.0;
/// `ceil(3600 / POLL_RATE)`: an initialized history always holds this many entries.
#[cfg(feature = "gated-tests")]
const HISTORY_LEN: usize = 720;

/// The instant every initialized history ends on. Earlier entries are zeroed copies of it,
/// `POLL_RATE` apart.
const INIT_AT: &str = "2026-10-25T00:59:30Z";
const HWMON_PUSHES: usize = 7;
const GPU_PUSHES: usize = 4;

/// Tick instants in UTC. They cover 0, 3, 6 and 9 fraction digits, straddle the end of
/// European summer time (01:00 UTC), and `T6` steps backwards like a wall clock reset.
const T1: &str = "2026-10-25T00:59:35.123Z";
const T2: &str = "2026-10-25T00:59:40.123456Z";
const T3: &str = "2026-10-25T00:59:45.123456789Z";
const T4: &str = "2026-10-25T00:59:59.5Z";
const T5: &str = "2026-10-25T01:00:04.5Z";
const T6: &str = "2026-10-25T00:59:58Z";
const T7: &str = "2026-10-25T01:00:09.25Z";

pub fn at(utc: &str) -> DateTime<Local> {
    utc.parse::<DateTime<Utc>>()
        .expect("fixture instants are RFC 3339")
        .with_timezone(&Local)
}

/// `since` lands exactly on `T4` once the status actor adds its 200 ms margin, so `T4` itself is
/// excluded and only entries strictly after it pass.
#[cfg(feature = "gated-tests")]
pub fn since() -> DateTime<Local> {
    at(T4) - TimeDelta::milliseconds(200)
}

fn temp(name: &str, temp: f64) -> TempStatus {
    TempStatus {
        name: name.to_string(),
        temp,
    }
}

fn channel(name: &str) -> ChannelStatus {
    ChannelStatus {
        name: name.to_string(),
        ..Default::default()
    }
}

fn fan(name: &str, rpm: u32, duty: Option<f64>) -> ChannelStatus {
    ChannelStatus {
        rpm: Some(rpm),
        duty,
        ..channel(name)
    }
}

fn with_pwm_mode(channel: ChannelStatus, pwm_mode: u8) -> ChannelStatus {
    ChannelStatus {
        pwm_mode: Some(pwm_mode),
        ..channel
    }
}

fn freq_watts(name: &str, freq: Option<u32>, watts: Option<f64>) -> ChannelStatus {
    ChannelStatus {
        freq,
        watts,
        ..channel(name)
    }
}

fn duty(name: &str, duty: f64) -> ChannelStatus {
    ChannelStatus {
        duty: Some(duty),
        ..channel(name)
    }
}

fn status(at_utc: &str, temps: Vec<TempStatus>, channels: Vec<ChannelStatus>) -> Status {
    Status {
        timestamp: at(at_utc),
        temps,
        channels,
    }
}

/// A hwmon-like device: names missing in some entries, a duty that only appears later, NaN,
/// names first seen late (`temp3`, `power`, `clock`, `led`), an entry without temps, and a
/// channel with only `pwm_mode`.
fn hwmon_statuses() -> (Status, Vec<Status>) {
    let init = status(
        INIT_AT,
        vec![temp("temp1", 40.0), temp("temp2", 41.5)],
        vec![fan("fan1", 1200, Some(50.0)), fan("pump", 2400, None)],
    );
    let mut pushes = hwmon_pushes_to_t4();
    pushes.extend(hwmon_pushes_after_t4());
    assert_eq!(pushes.len(), HWMON_PUSHES);
    (init, pushes)
}

fn hwmon_pushes_to_t4() -> Vec<Status> {
    vec![
        status(
            T1,
            vec![temp("temp1", 40.25), temp("temp2", f64::NAN)],
            vec![
                with_pwm_mode(fan("fan1", 1210, Some(51.0)), 1),
                fan("pump", 2410, None),
            ],
        ),
        status(
            T2,
            vec![temp("temp1", 40.5)],
            vec![fan("fan1", 1220, Some(52.0)), fan("pump", 2420, Some(60.0))],
        ),
        status(
            T3,
            vec![
                temp("temp1", 40.75),
                temp("temp2", 42.0),
                temp("temp3", 30.0),
            ],
            vec![
                fan("fan1", 1230, None),
                fan("pump", 2430, Some(61.0)),
                freq_watts("power", Some(1500), Some(12.5)),
            ],
        ),
        status(
            T4,
            vec![
                temp("temp1", 41.0),
                temp("temp2", 42.25),
                temp("temp3", 30.5),
            ],
            vec![
                fan("fan1", 1240, Some(53.0)),
                fan("pump", 2440, Some(62.0)),
                freq_watts("power", Some(1510), Some(13.0)),
            ],
        ),
    ]
}

fn hwmon_pushes_after_t4() -> Vec<Status> {
    vec![
        status(
            T5,
            vec![temp("temp1", 41.25), temp("temp3", 31.0)],
            vec![fan("fan1", 1250, Some(53.5)), fan("pump", 2450, Some(62.5))],
        ),
        status(
            T6,
            vec![],
            vec![fan("fan1", 1255, Some(33.333_333_333_333_336))],
        ),
        status(
            T7,
            vec![
                temp("temp1", 41.5),
                temp("temp2", f64::NAN),
                temp("temp3", 31.5),
            ],
            vec![
                with_pwm_mode(fan("fan1", 1260, Some(54.0)), 1),
                fan("pump", 2460, None),
                freq_watts("power", Some(1520), Some(13.5)),
                freq_watts("clock", Some(4200), None),
                with_pwm_mode(channel("led"), 2),
            ],
        ),
    ]
}

/// A GPU-like device with no temps that skips ticks `T2`, `T5` and `T6` and reports nothing on
/// `T3`.
fn gpu_statuses() -> (Status, Vec<Status>) {
    let init = gpu_status(INIT_AT, 10.0, 3000, Some(55.5));
    let pushes = vec![
        gpu_status(T1, 11.5, 3010, Some(56.0)),
        status(T3, vec![], vec![]),
        gpu_status(T4, 12.0, 3020, None),
        gpu_status(T7, 33.333_333_333_333_336, 3030, Some(55.75)),
    ];
    assert_eq!(pushes.len(), GPU_PUSHES);
    (init, pushes)
}

/// A GPU tick reporting `load` and `freq`, and `power` when `watts` is given.
fn gpu_status(at_utc: &str, load: f64, freq: u32, watts: Option<f64>) -> Status {
    let mut channels = Vec::with_capacity(3);
    channels.push(duty("load", load));
    channels.push(freq_watts("freq", Some(freq), None));
    if let Some(watts) = watts {
        channels.push(freq_watts("power", None, Some(watts)));
    }
    status(at_utc, vec![], channels)
}

fn device_with(name: &str, d_type: DeviceType, statuses: (Status, Vec<Status>)) -> Device {
    let mut device = Device::new(
        name.to_string(),
        d_type,
        1,
        None,
        DeviceInfo::default(),
        Some(format!("golden-{name}")),
        POLL_RATE,
    );
    let (init, pushes) = statuses;
    device.initialize_status_history_with(init, POLL_RATE);
    for status in pushes {
        device.set_status(status);
    }
    device
}

pub struct Fixture {
    pub all_devices: AllDevices,
    pub hwmon_uid: UID,
    pub gpu_uid: UID,
}

pub fn fixture() -> Fixture {
    let hwmon = device_with("Golden Hwmon", DeviceType::Hwmon, hwmon_statuses());
    let gpu = device_with("Golden GPU", DeviceType::GPU, gpu_statuses());
    let hwmon_uid = hwmon.uid.clone();
    let gpu_uid = gpu.uid.clone();
    let mut devices = HashMap::with_capacity(2);
    devices.insert(hwmon_uid.clone(), Rc::new(RefCell::new(hwmon)));
    devices.insert(gpu_uid.clone(), Rc::new(RefCell::new(gpu)));
    Fixture {
        all_devices: Rc::new(devices),
        hwmon_uid,
        gpu_uid,
    }
}

/// What `/devices` reports for the fixture, for the Prometheus labels.
pub fn device_dtos(fixture: &Fixture) -> Vec<DeviceDto> {
    fixture
        .all_devices
        .values()
        .map(|device| DeviceDto::from(&*device.borrow()))
        .collect()
}

/// Every status query the API offers, answered by a real status actor over the fixture.
pub struct Responses {
    #[cfg(feature = "gated-tests")]
    pub all: Vec<DeviceStatusDto>,
    #[cfg(feature = "gated-tests")]
    pub since: Vec<DeviceStatusDto>,
    pub recent: Vec<DeviceStatusDto>,
    #[cfg(feature = "gated-tests")]
    pub hwmon_all: DeviceStatusDto,
    pub hwmon_recent: DeviceStatusDto,
    #[cfg(feature = "gated-tests")]
    pub pump_all: DeviceChannelStatusDto,
    #[cfg(feature = "gated-tests")]
    pub temp2_all: DeviceChannelStatusDto,
    #[cfg(feature = "gated-tests")]
    pub power_recent: DeviceChannelStatusDto,
    #[cfg(feature = "gated-tests")]
    pub unknown_recent: DeviceChannelStatusDto,
}

pub fn responses(fixture: &Fixture) -> Responses {
    let answered: RefCell<Option<Responses>> = RefCell::new(None);
    let answered_ref = &answered;
    crate::rt::test_runtime(async {
        let cancel_token = CancellationToken::new();
        moro_local::async_scope!(|main_scope| -> anyhow::Result<()> {
            let handle = StatusHandle::new(
                Rc::clone(&fixture.all_devices),
                cancel_token.clone(),
                main_scope,
            );
            let hwmon = fixture.hwmon_uid.clone();
            let responses = Responses {
                #[cfg(feature = "gated-tests")]
                all: handle.all().await?,
                #[cfg(feature = "gated-tests")]
                since: handle.since(since()).await?,
                recent: handle.recent().await?,
                #[cfg(feature = "gated-tests")]
                hwmon_all: handle.all_device(hwmon.clone()).await?,
                hwmon_recent: handle.recent_device(hwmon.clone()).await?,
                #[cfg(feature = "gated-tests")]
                pump_all: handle
                    .all_device_channel(hwmon.clone(), "pump".to_string())
                    .await?,
                #[cfg(feature = "gated-tests")]
                temp2_all: handle
                    .all_device_channel(hwmon.clone(), "temp2".to_string())
                    .await?,
                #[cfg(feature = "gated-tests")]
                power_recent: handle
                    .recent_device_channel(hwmon.clone(), "power".to_string())
                    .await?,
                #[cfg(feature = "gated-tests")]
                unknown_recent: handle
                    .recent_device_channel(hwmon, "unknown".to_string())
                    .await?,
            };
            answered_ref.borrow_mut().replace(responses);
            // Stops the actor so the scope can finish.
            cancel_token.cancel();
            Ok(())
        })
        .await
        .expect("the status actor answers");
    });
    answered.into_inner().expect("every query was answered")
}

// ---------------------------------------------------------------------------------------------
// Expected bytes
// ---------------------------------------------------------------------------------------------

/// Today's rendering of an instant: chrono's serialization of the `DateTime<Local>` the
/// repositories stamp.
#[cfg(feature = "gated-tests")]
fn timestamp_json(at: DateTime<Local>) -> String {
    serde_json::to_string(&at).expect("timestamps serialize")
}

#[cfg(feature = "gated-tests")]
fn entry_json(at: DateTime<Local>, body: &str) -> String {
    let timestamp = timestamp_json(at);
    if body.is_empty() {
        format!("{{\"timestamp\":{timestamp}}}")
    } else {
        format!("{{\"timestamp\":{timestamp},{body}}}")
    }
}

/// The zeroed entries `initialize_status_history_with` puts before the init entry, oldest first.
#[cfg(feature = "gated-tests")]
fn zeroed_entries(pushes: usize, body: &str) -> Vec<String> {
    let zeroed = HISTORY_LEN - 1 - pushes;
    let init_at = at(INIT_AT);
    (1..=zeroed)
        .rev()
        .map(|position| {
            let offset = TimeDelta::seconds(5 * i64::try_from(position).unwrap());
            entry_json(init_at - offset, body)
        })
        .collect()
}

#[cfg(feature = "gated-tests")]
const HWMON_ZEROED: &str = r#""temps":[{"name":"temp1","temp":0.0},{"name":"temp2","temp":0.0}],"channels":[{"name":"fan1","rpm":0,"duty":0.0},{"name":"pump","rpm":0}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_INIT: &str = r#""temps":[{"name":"temp1","temp":40.0},{"name":"temp2","temp":41.5}],"channels":[{"name":"fan1","rpm":1200,"duty":50.0},{"name":"pump","rpm":2400}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T1: &str = r#""temps":[{"name":"temp1","temp":40.25},{"name":"temp2","temp":null}],"channels":[{"name":"fan1","rpm":1210,"duty":51.0,"pwm_mode":1},{"name":"pump","rpm":2410}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T2: &str = r#""temps":[{"name":"temp1","temp":40.5}],"channels":[{"name":"fan1","rpm":1220,"duty":52.0},{"name":"pump","rpm":2420,"duty":60.0}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T3: &str = r#""temps":[{"name":"temp1","temp":40.75},{"name":"temp2","temp":42.0},{"name":"temp3","temp":30.0}],"channels":[{"name":"fan1","rpm":1230},{"name":"pump","rpm":2430,"duty":61.0},{"name":"power","freq":1500,"watts":12.5}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T4: &str = r#""temps":[{"name":"temp1","temp":41.0},{"name":"temp2","temp":42.25},{"name":"temp3","temp":30.5}],"channels":[{"name":"fan1","rpm":1240,"duty":53.0},{"name":"pump","rpm":2440,"duty":62.0},{"name":"power","freq":1510,"watts":13.0}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T5: &str = r#""temps":[{"name":"temp1","temp":41.25},{"name":"temp3","temp":31.0}],"channels":[{"name":"fan1","rpm":1250,"duty":53.5},{"name":"pump","rpm":2450,"duty":62.5}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T6: &str = r#""channels":[{"name":"fan1","rpm":1255,"duty":33.333333333333336}]"#;
#[cfg(feature = "gated-tests")]
const HWMON_T7: &str = r#""temps":[{"name":"temp1","temp":41.5},{"name":"temp2","temp":null},{"name":"temp3","temp":31.5}],"channels":[{"name":"fan1","rpm":1260,"duty":54.0,"pwm_mode":1},{"name":"pump","rpm":2460},{"name":"power","freq":1520,"watts":13.5},{"name":"clock","freq":4200},{"name":"led","pwm_mode":2}]"#;

#[cfg(feature = "gated-tests")]
const GPU_ZEROED: &str = r#""channels":[{"name":"load","duty":0.0},{"name":"freq","freq":0},{"name":"power","watts":0.0}]"#;
#[cfg(feature = "gated-tests")]
const GPU_INIT: &str = r#""channels":[{"name":"load","duty":10.0},{"name":"freq","freq":3000},{"name":"power","watts":55.5}]"#;
#[cfg(feature = "gated-tests")]
const GPU_T1: &str = r#""channels":[{"name":"load","duty":11.5},{"name":"freq","freq":3010},{"name":"power","watts":56.0}]"#;
#[cfg(feature = "gated-tests")]
const GPU_T3: &str = "";
#[cfg(feature = "gated-tests")]
const GPU_T4: &str = r#""channels":[{"name":"load","duty":12.0},{"name":"freq","freq":3020}]"#;
#[cfg(feature = "gated-tests")]
const GPU_T7: &str = r#""channels":[{"name":"load","duty":33.333333333333336},{"name":"freq","freq":3030},{"name":"power","watts":55.75}]"#;

#[cfg(feature = "gated-tests")]
fn hwmon_entries() -> Vec<String> {
    let mut entries = zeroed_entries(HWMON_PUSHES, HWMON_ZEROED);
    for (at_utc, body) in [
        (INIT_AT, HWMON_INIT),
        (T1, HWMON_T1),
        (T2, HWMON_T2),
        (T3, HWMON_T3),
        (T4, HWMON_T4),
        (T5, HWMON_T5),
        (T6, HWMON_T6),
        (T7, HWMON_T7),
    ] {
        entries.push(entry_json(at(at_utc), body));
    }
    assert_eq!(entries.len(), HISTORY_LEN);
    entries
}

#[cfg(feature = "gated-tests")]
fn gpu_entries() -> Vec<String> {
    let mut entries = zeroed_entries(GPU_PUSHES, GPU_ZEROED);
    for (at_utc, body) in [
        (INIT_AT, GPU_INIT),
        (T1, GPU_T1),
        (T3, GPU_T3),
        (T4, GPU_T4),
        (T7, GPU_T7),
    ] {
        entries.push(entry_json(at(at_utc), body));
    }
    assert_eq!(entries.len(), HISTORY_LEN);
    entries
}

#[cfg(feature = "gated-tests")]
fn device_json(d_type: &str, uid: &str, entries: &[String]) -> String {
    format!(
        "{{\"type\":\"{d_type}\",\"type_index\":1,\"uid\":\"{uid}\",\"status_history\":[{}]}}",
        entries.join(",")
    )
}

#[cfg(feature = "gated-tests")]
fn channel_json(entries: &[String]) -> String {
    format!("{{\"status_history\":[{}]}}", entries.join(","))
}

/// Byte equality with a readable report: a 720-entry mismatch otherwise prints two walls of JSON.
#[track_caller]
#[cfg(feature = "gated-tests")]
fn assert_same_bytes(actual: &str, expected: &str, what: &str) {
    if actual == expected {
        return;
    }
    let first_difference = actual
        .bytes()
        .zip(expected.bytes())
        .position(|(a, e)| a != e)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    let window = |text: &str| {
        let start = first_difference.saturating_sub(120);
        let end = (first_difference + 120).min(text.len());
        text.get(start..end).unwrap_or(text).to_string()
    };
    panic!(
        "{what} differs at byte {first_difference} (actual {} bytes, expected {} bytes)\n\
         actual:   ...{}...\nexpected: ...{}...",
        actual.len(),
        expected.len(),
        window(actual),
        window(expected),
    );
}

#[cfg(feature = "gated-tests")]
fn by_uid<'a>(devices: &'a [DeviceStatusDto], uid: &str) -> &'a DeviceStatusDto {
    devices
        .iter()
        .find(|device| device.uid == uid)
        .expect("every fixture device answers")
}

#[cfg(feature = "gated-tests")]
fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("status DTOs serialize")
}

/// Whether the actor answered with the hwmon device first. `AllDevices` iteration order is not
/// part of the contract, so the expected response follows it.
#[cfg(feature = "gated-tests")]
fn hwmon_first(devices: &[DeviceStatusDto], fixture: &Fixture) -> bool {
    assert_eq!(devices.len(), 2, "one answer per fixture device");
    devices[0].uid == fixture.hwmon_uid
}

#[cfg(feature = "gated-tests")]
fn response_json(hwmon_first: bool, hwmon: &str, gpu: &str) -> String {
    if hwmon_first {
        format!("{{\"devices\":[{hwmon},{gpu}]}}")
    } else {
        format!("{{\"devices\":[{gpu},{hwmon}]}}")
    }
}

#[track_caller]
#[cfg(feature = "gated-tests")]
fn assert_response(
    fixture: &Fixture,
    actual: Vec<DeviceStatusDto>,
    hwmon: &str,
    gpu: &str,
    what: &str,
) {
    let expected = response_json(hwmon_first(&actual, fixture), hwmon, gpu);
    assert_same_bytes(
        &to_json(&StatusResponse { devices: actual }),
        &expected,
        what,
    );
}

#[test]
#[cfg(feature = "gated-tests")]
fn fixture_timestamps_cover_every_precision() {
    // Goal: the fixture really exercises every timestamp shape chrono can print, so the
    // golden tests below pin all of them. Method: count the fraction digits of each expected
    // timestamp string; the offset suffix is zone dependent and is left out.
    let fraction_digits = |utc: &str| {
        let rendered = timestamp_json(at(utc));
        let seconds_end = rendered.find(':').unwrap() + 6;
        let after_seconds = &rendered[seconds_end..];
        after_seconds.strip_prefix('.').map_or(0, |fraction| {
            fraction.bytes().take_while(u8::is_ascii_digit).count()
        })
    };
    let digits: Vec<usize> = [INIT_AT, T1, T2, T3, T4, T5, T6, T7]
        .into_iter()
        .map(fraction_digits)
        .collect();
    assert_eq!(digits, vec![0, 3, 6, 9, 3, 3, 0, 3]);
}

#[test]
#[cfg(feature = "gated-tests")]
fn rest_all_is_golden() {
    // Goal: `{all: true}` returns every entry of every device byte for byte. Method: compare
    // the response with the expected JSON in both device orders.
    let fixture = fixture();
    let responses = responses(&fixture);
    let hwmon = device_json("Hwmon", &fixture.hwmon_uid, &hwmon_entries());
    let gpu = device_json("GPU", &fixture.gpu_uid, &gpu_entries());
    assert_response(&fixture, responses.all, &hwmon, &gpu, "all");
}

#[test]
#[cfg(feature = "gated-tests")]
fn rest_since_keeps_entries_strictly_after_the_margin() {
    // Goal: `since` keeps exactly the entries stamped after `since + 200 ms`, judged entry by
    // entry, so `T6` (a wall clock step back) is dropped even though a later entry passes.
    // Method: `since + 200 ms` equals `T4`, which must be excluded too.
    let fixture = fixture();
    let responses = responses(&fixture);
    let hwmon = device_json(
        "Hwmon",
        &fixture.hwmon_uid,
        &[entry_json(at(T5), HWMON_T5), entry_json(at(T7), HWMON_T7)],
    );
    let gpu = device_json("GPU", &fixture.gpu_uid, &[entry_json(at(T7), GPU_T7)]);
    assert_response(&fixture, responses.since, &hwmon, &gpu, "since");
}

#[test]
#[cfg(feature = "gated-tests")]
fn rest_recent_is_golden() {
    // Goal: the default query returns only each device's newest entry. Method: compare bytes.
    let fixture = fixture();
    let responses = responses(&fixture);
    let hwmon = device_json("Hwmon", &fixture.hwmon_uid, &[entry_json(at(T7), HWMON_T7)]);
    let gpu = device_json("GPU", &fixture.gpu_uid, &[entry_json(at(T7), GPU_T7)]);
    assert_response(&fixture, responses.recent, &hwmon, &gpu, "recent");
}

#[test]
#[cfg(feature = "gated-tests")]
fn rest_device_all_and_recent_are_golden() {
    // Goal: the per-device routes return the same entries as the all-device ones. Method:
    // compare bytes for the hwmon device.
    let fixture = fixture();
    let responses = responses(&fixture);
    assert_same_bytes(
        &to_json(&responses.hwmon_all),
        &device_json("Hwmon", &fixture.hwmon_uid, &hwmon_entries()),
        "device all",
    );
    assert_same_bytes(
        &to_json(&responses.hwmon_recent),
        &device_json("Hwmon", &fixture.hwmon_uid, &[entry_json(at(T7), HWMON_T7)]),
        "device recent",
    );
    // The all-device answer agrees with the per-device one.
    assert_same_bytes(
        &to_json(by_uid(&responses.all, &fixture.hwmon_uid)),
        &to_json(&responses.hwmon_all),
        "all vs device all",
    );
}

#[test]
#[cfg(feature = "gated-tests")]
fn rest_channel_all_and_recent_are_golden() {
    // Goal: the per-channel routes keep every entry's timestamp and only the named temp or
    // channel, which is omitted where it was not reported. An unknown name keeps bare
    // timestamps. Method: compare bytes for a channel, a temp, the newest entry and an
    // unknown name.
    let fixture = fixture();
    let responses = responses(&fixture);

    let pump_zeroed = r#""channels":[{"name":"pump","rpm":0}]"#;
    let mut pump = zeroed_entries(HWMON_PUSHES, pump_zeroed);
    for (at_utc, body) in [
        (INIT_AT, r#""channels":[{"name":"pump","rpm":2400}]"#),
        (T1, r#""channels":[{"name":"pump","rpm":2410}]"#),
        (T2, r#""channels":[{"name":"pump","rpm":2420,"duty":60.0}]"#),
        (T3, r#""channels":[{"name":"pump","rpm":2430,"duty":61.0}]"#),
        (T4, r#""channels":[{"name":"pump","rpm":2440,"duty":62.0}]"#),
        (T5, r#""channels":[{"name":"pump","rpm":2450,"duty":62.5}]"#),
        (T6, ""),
        (T7, r#""channels":[{"name":"pump","rpm":2460}]"#),
    ] {
        pump.push(entry_json(at(at_utc), body));
    }
    assert_same_bytes(
        &to_json(&responses.pump_all),
        &channel_json(&pump),
        "pump all",
    );

    let temp2_zeroed = r#""temps":[{"name":"temp2","temp":0.0}]"#;
    let mut temp2 = zeroed_entries(HWMON_PUSHES, temp2_zeroed);
    for (at_utc, body) in [
        (INIT_AT, r#""temps":[{"name":"temp2","temp":41.5}]"#),
        (T1, r#""temps":[{"name":"temp2","temp":null}]"#),
        (T2, ""),
        (T3, r#""temps":[{"name":"temp2","temp":42.0}]"#),
        (T4, r#""temps":[{"name":"temp2","temp":42.25}]"#),
        (T5, ""),
        (T6, ""),
        (T7, r#""temps":[{"name":"temp2","temp":null}]"#),
    ] {
        temp2.push(entry_json(at(at_utc), body));
    }
    assert_same_bytes(
        &to_json(&responses.temp2_all),
        &channel_json(&temp2),
        "temp2 all",
    );

    assert_same_bytes(
        &to_json(&responses.power_recent),
        &channel_json(&[entry_json(
            at(T7),
            r#""channels":[{"name":"power","freq":1520,"watts":13.5}]"#,
        )]),
        "power recent",
    );
    assert_same_bytes(
        &to_json(&responses.unknown_recent),
        &channel_json(&[entry_json(at(T7), "")]),
        "unknown recent",
    );
}

#[test]
#[cfg(feature = "gated-tests")]
fn sse_status_frame_is_golden() {
    // Goal: the SSE `status` frame carries the recent response unchanged. Method: render the
    // frame through axum's SSE body and compare the wire bytes in both device orders.
    let fixture = fixture();
    let responses = responses(&fixture);
    let hwmon_first = hwmon_first(&responses.recent, &fixture);
    let event: Event = SseEvent::Status(StatusResponse {
        devices: responses.recent,
    })
    .into();
    let stream = futures_util::stream::iter([Ok::<_, std::convert::Infallible>(event)]);
    let wire = crate::rt::test_runtime(async {
        let body = Sse::new(stream).into_response().into_body();
        axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("a finite SSE body collects")
    });
    let wire = String::from_utf8(wire.to_vec()).expect("SSE frames are UTF-8");
    let hwmon = device_json("Hwmon", &fixture.hwmon_uid, &[entry_json(at(T7), HWMON_T7)]);
    let gpu = device_json("GPU", &fixture.gpu_uid, &[entry_json(at(T7), GPU_T7)]);
    let data = response_json(hwmon_first, &hwmon, &gpu);
    assert_same_bytes(
        &wire,
        &format!("event: status\ndata: {data}\n\n"),
        "sse status frame",
    );
}

#[test]
fn grpc_status_is_golden() {
    // Goal: gRPC `Status` lists the newest entry's temps then channels in entry order, and picks
    // one metric per channel: speed if duty or rpm is present, else watts, else MHz, else none.
    // Method: compare the Debug rendering, which also matches NaN.
    let fixture = fixture();
    let responses = responses(&fixture);
    let actual: Vec<models::v1::Status> = responses.hwmon_recent.into();
    let grpc = |id: &str, metric: Option<Metric>| models::v1::Status {
        id: id.to_string(),
        metric,
    };
    let expected = vec![
        grpc("temp1", Some(Metric::Temp(41.5))),
        grpc("temp2", Some(Metric::Temp(f64::NAN))),
        grpc("temp3", Some(Metric::Temp(31.5))),
        grpc(
            "fan1",
            Some(Metric::Speed(FanSpeed {
                duty: Some(54.0),
                rpm: Some(1260),
            })),
        ),
        grpc(
            "pump",
            Some(Metric::Speed(FanSpeed {
                duty: None,
                rpm: Some(2460),
            })),
        ),
        grpc("power", Some(Metric::Watts(13.5))),
        grpc("clock", Some(Metric::Mhz(4200))),
        grpc("led", None),
    ];
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}
