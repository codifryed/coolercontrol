/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::HashMap;
use std::fmt::Write;

use aide::axum::IntoApiResponse;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;

use crate::api::devices::DeviceDto;
use crate::api::status::DeviceStatusDto;
use crate::api::{AppState, CCError};
use crate::device::Status;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

pub async fn get_metrics(
    State(AppState {
        device_handle,
        status_handle,
        ..
    }): State<AppState>,
) -> Result<impl IntoApiResponse, CCError> {
    let (devices_result, statuses_result) =
        tokio::join!(device_handle.devices_get(), status_handle.recent(),);
    let devices = devices_result?;
    let statuses = statuses_result?;
    let body = format_prometheus_metrics(&devices, &statuses);
    Ok(([(CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], body))
}

fn format_prometheus_metrics(devices: &[DeviceDto], statuses: &[DeviceStatusDto]) -> String {
    let device_map: HashMap<&str, &DeviceDto> =
        devices.iter().map(|d| (d.uid.as_str(), d)).collect();

    let mut out = String::with_capacity(4096);

    write_header(
        &mut out,
        "coolercontrol_temperature_celsius",
        "Temperature sensor reading in degrees Celsius.",
        "gauge",
    );
    write_header(
        &mut out,
        "coolercontrol_fan_rpm",
        "Fan speed in revolutions per minute.",
        "gauge",
    );
    write_header(
        &mut out,
        "coolercontrol_duty_percent",
        "Channel duty cycle or load percentage (0-100).",
        "gauge",
    );
    write_header(
        &mut out,
        "coolercontrol_frequency_hertz",
        "Channel frequency in Hertz.",
        "gauge",
    );
    write_header(
        &mut out,
        "coolercontrol_power_watts",
        "Channel power consumption in Watts.",
        "gauge",
    );

    for status in statuses {
        let Some(device) = device_map.get(status.uid.as_str()) else {
            continue;
        };
        let Some(recent) = status.status_history.back() else {
            continue;
        };
        write_temp_metrics(&mut out, device, recent);
        write_channel_metrics(&mut out, device, recent);
    }

    out
}

fn write_temp_metrics(out: &mut String, device: &DeviceDto, status: &Status) {
    for temp in &status.temps {
        let _ = writeln!(
            out,
            "coolercontrol_temperature_celsius{{device=\"{}\",device_uid=\"{}\",device_type=\"{}\",sensor=\"{}\"}} {}",
            escape_label_value(&device.name),
            device.uid,
            device.d_type,
            escape_label_value(&temp.name),
            temp.temp,
        );
    }
}

fn write_channel_metrics(out: &mut String, device: &DeviceDto, status: &Status) {
    for ch in &status.channels {
        let dev = escape_label_value(&device.name);
        let channel = escape_label_value(&ch.name);
        if let Some(rpm) = ch.rpm {
            let _ = writeln!(
                out,
                "coolercontrol_fan_rpm{{device=\"{dev}\",device_uid=\"{}\",device_type=\"{}\",channel=\"{channel}\"}} {rpm}",
                device.uid, device.d_type,
            );
        }
        if let Some(duty) = ch.duty {
            let _ = writeln!(
                out,
                "coolercontrol_duty_percent{{device=\"{dev}\",device_uid=\"{}\",device_type=\"{}\",channel=\"{channel}\"}} {duty}",
                device.uid, device.d_type,
            );
        }
        if let Some(mhz) = ch.freq {
            let hz = u64::from(mhz) * 1_000_000;
            let _ = writeln!(
                out,
                "coolercontrol_frequency_hertz{{device=\"{dev}\",device_uid=\"{}\",device_type=\"{}\",channel=\"{channel}\"}} {hz}",
                device.uid, device.d_type,
            );
        }
        if let Some(watts) = ch.watts {
            let _ = writeln!(
                out,
                "coolercontrol_power_watts{{device=\"{dev}\",device_uid=\"{}\",device_type=\"{}\",channel=\"{channel}\"}} {watts}",
                device.uid, device.d_type,
            );
        }
    }
}

fn write_header(out: &mut String, name: &str, help: &str, metric_type: &str) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {metric_type}");
}

fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::devices::DeviceDto;
    use crate::api::status::DeviceStatusDto;
    use crate::device::{ChannelStatus, DeviceInfo, DeviceType, Status, TempStatus};
    use std::collections::VecDeque;
    use std::sync::Arc;

    fn make_device(name: &str, uid: &str, d_type: DeviceType) -> DeviceDto {
        DeviceDto {
            name: name.to_string(),
            d_type,
            type_index: 0,
            uid: uid.to_string(),
            lc_info: None,
            info: DeviceInfo::default(),
        }
    }

    fn make_status(
        uid: &str,
        temps: Vec<TempStatus>,
        channels: Vec<ChannelStatus>,
    ) -> DeviceStatusDto {
        let mut history = VecDeque::new();
        history.push_back(Status {
            timestamp: chrono::Local::now(),
            temps,
            channels,
        });
        DeviceStatusDto {
            d_type: DeviceType::Hwmon,
            type_index: 0,
            uid: uid.to_string(),
            status_history: Arc::new(history),
        }
    }

    #[test]
    fn test_empty_data() {
        let output = format_prometheus_metrics(&[], &[]);
        assert!(output.contains("# HELP coolercontrol_temperature_celsius"));
        assert!(output.contains("# TYPE coolercontrol_temperature_celsius gauge"));
    }

    #[test]
    fn test_temperature_metric() {
        let devices = vec![make_device("Test Hwmon", "uid1", DeviceType::Hwmon)];
        let statuses = vec![make_status(
            "uid1",
            vec![TempStatus {
                name: "CPU Temp".to_string(),
                temp: 55.5,
            }],
            vec![],
        )];
        let output = format_prometheus_metrics(&devices, &statuses);
        assert!(output.contains(
            r#"coolercontrol_temperature_celsius{device="Test Hwmon",device_uid="uid1",device_type="Hwmon",sensor="CPU Temp"} 55.5"#
        ));
    }

    #[test]
    fn test_channel_metrics() {
        let devices = vec![make_device("GPU Card", "uid2", DeviceType::GPU)];
        let statuses = vec![make_status(
            "uid2",
            vec![],
            vec![ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(45.0),
                freq: Some(1800),
                watts: Some(120.5),
                pwm_mode: None,
            }],
        )];
        let output = format_prometheus_metrics(&devices, &statuses);
        assert!(output.contains(
            r#"coolercontrol_fan_rpm{device="GPU Card",device_uid="uid2",device_type="GPU",channel="fan1"} 1200"#
        ));
        assert!(output.contains(
            r#"coolercontrol_duty_percent{device="GPU Card",device_uid="uid2",device_type="GPU",channel="fan1"} 45"#
        ));
        assert!(output.contains(
            r#"coolercontrol_frequency_hertz{device="GPU Card",device_uid="uid2",device_type="GPU",channel="fan1"} 1800000000"#
        ));
        assert!(output.contains(
            r#"coolercontrol_power_watts{device="GPU Card",device_uid="uid2",device_type="GPU",channel="fan1"} 120.5"#
        ));
    }

    #[test]
    fn test_escape_label_value() {
        assert_eq!(escape_label_value(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape_label_value("line\nnewline"), "line\\nnewline");
    }

    #[test]
    fn test_missing_device_skipped() {
        let devices = vec![];
        let statuses = vec![make_status(
            "orphan",
            vec![TempStatus {
                name: "temp".to_string(),
                temp: 30.0,
            }],
            vec![],
        )];
        let output = format_prometheus_metrics(&devices, &statuses);
        assert!(!output.contains("orphan"));
    }

    #[test]
    fn test_none_channels_skipped() {
        let devices = vec![make_device("Dev", "uid3", DeviceType::CPU)];
        let statuses = vec![make_status(
            "uid3",
            vec![],
            vec![ChannelStatus {
                name: "ch".to_string(),
                rpm: None,
                duty: None,
                freq: None,
                watts: None,
                pwm_mode: None,
            }],
        )];
        let output = format_prometheus_metrics(&devices, &statuses);
        assert!(!output.contains("coolercontrol_fan_rpm{"));
        assert!(!output.contains("coolercontrol_duty_percent{"));
        assert!(!output.contains("coolercontrol_frequency_hertz{"));
        assert!(!output.contains("coolercontrol_power_watts{"));
    }
}
