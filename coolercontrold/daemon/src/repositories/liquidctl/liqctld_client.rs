// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The JSON bodies liqctld exchanges. The calls that send them are in `liqctld_io`.
use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A standard liquidctl status response (name, value, metric).
pub type LCStatus = Vec<(String, String, SkippedUnit)>;

/// A status line's metric, skipped while parsing: nothing reads it, and every line of every
/// device would otherwise allocate it each tick. Serializes as an empty string, keeping the
/// three-element shape liqctld sends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkippedUnit;

impl<'de> Deserialize<'de> for SkippedUnit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
}

impl Serialize for SkippedUnit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceResponse {
    pub id: u8,
    pub description: String,
    /// Also called `DriverName`
    pub device_type: String,
    pub serial_number: Option<String>,
    pub properties: DeviceProperties,
    pub liquidctl_version: Option<String>,
    pub hid_address: Option<String>,
    pub hwmon_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProperties {
    pub speed_channels: Vec<String>,
    pub color_channels: Vec<String>,
    pub supports_cooling: Option<bool>,
    pub supports_cooling_profiles: Option<bool>,
    pub supports_lighting: Option<bool>,
    pub led_count: Option<u8>,
    pub lcd_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: LCStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    pub pump_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedSpeedRequest {
    pub channel: String,
    pub duty: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedProfileRequest {
    pub channel: String,
    // INFO: Some liquidctl device drivers need ints. coolercontrol-liqctld will handle this.
    pub profile: Vec<(f64, u8)>,
    pub temperature_sensor: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlModeRequest {
    pub channel: String,
    pub desired_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorRequest {
    pub channel: String,
    pub mode: String,
    pub colors: Vec<(u8, u8, u8)>,
    pub time_per_color: Option<u8>,
    pub speed: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenRequest {
    pub channel: String,
    pub mode: String,
    pub value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three lines as liqctld's Python `json.dumps` writes them, with the escaped degree sign.
    const STATUS_BODY: &str = r#"{"status": [["Liquid temperature", "31.5", "°C"],
        ["Pump speed", "2000", "rpm"], ["Pump duty", "70", "%"]]}"#;

    /// Goal: a status reply parses to its names and values with the metric skipped, and a
    /// parsed line still writes back in liqctld's three-element shape. Method: parse a reply as
    /// liqctld sends it, check what landed, and serialize one line again.
    #[test]
    fn status_lines_parse_without_their_metric() {
        let parsed: StatusResponse = serde_json::from_str(STATUS_BODY).unwrap();

        assert_eq!(parsed.status.len(), 3);
        assert_eq!(parsed.status[0].0, "Liquid temperature");
        assert_eq!(
            parsed.status[1],
            ("Pump speed".to_string(), "2000".to_string(), SkippedUnit)
        );
        let written = serde_json::to_string(&parsed.status[2]).unwrap();
        assert_eq!(written, r#"["Pump duty","70",""]"#);
    }

    /// Goal: a status line costs its name and value only, every tick, on every liquidctl worker.
    /// Method: count the allocations of parsing a three-line reply.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_status_line_allocates_only_its_name_and_value() {
        let (parsed, allocations) = crate::alloc_count::count_allocations(|| {
            serde_json::from_str::<StatusResponse>(STATUS_BODY).unwrap()
        });

        assert_eq!(parsed.status.len(), 3);
        // Two strings a line, plus the vector, whose first push sizes it for four lines.
        assert_eq!(allocations, 2 * 3 + 1);
    }
}
