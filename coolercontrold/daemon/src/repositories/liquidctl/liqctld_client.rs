// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The JSON bodies liqctld exchanges. The calls that send them are in `liqctld_io`.
use std::borrow::Cow;
use std::fmt;
use std::ops::Not;

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
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

/// Parses a status reply into `lines`, reusing their buffers, so a reply with the lines of the
/// last writes into their allocations rather than making its own. Text is borrowed from
/// `body` while parsing, and decoded into an owned string only where liqctld escaped it.
///
/// # Errors
///
/// When `body` is not a status reply, as `serde_json::from_slice::<StatusResponse>` would.
pub fn parse_status_into(body: &[u8], lines: &mut LCStatus) -> serde_json::Result<()> {
    let mut deserializer = serde_json::Deserializer::from_slice(body);
    StatusSeed(lines).deserialize(&mut deserializer)?;
    deserializer.end()
}

/// A string borrowed from the reply when it holds no escapes.
#[derive(Deserialize)]
struct Text<'a>(#[serde(borrow)] Cow<'a, str>);

/// The reply object, of which only `status` is read.
struct StatusSeed<'a>(&'a mut LCStatus);

impl<'de> DeserializeSeed<'de> for StatusSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for StatusSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a liqctld status reply")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let lines = self.0;
        let mut seen = false;
        while let Some(key) = map.next_key::<Text<'de>>()? {
            if key.0 != "status" {
                map.next_value::<IgnoredAny>()?;
                continue;
            }
            if seen {
                return Err(de::Error::duplicate_field("status"));
            }
            map.next_value_seed(LinesSeed(&mut *lines))?;
            seen = true;
        }
        if seen.not() {
            return Err(de::Error::missing_field("status"));
        }
        Ok(())
    }
}

/// The status lines, each written over the line at its position.
struct LinesSeed<'a>(&'a mut LCStatus);

impl<'de> DeserializeSeed<'de> for LinesSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for LinesSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a list of [name, value, unit] status lines")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let lines = self.0;
        let mut count = 0;
        while let Some((name, value, unit)) =
            seq.next_element::<(Text<'de>, Text<'de>, SkippedUnit)>()?
        {
            match lines.get_mut(count) {
                Some(line) => {
                    rewrite(&mut line.0, &name.0);
                    rewrite(&mut line.1, &value.0);
                }
                None => lines.push((name.0.into_owned(), value.0.into_owned(), unit)),
            }
            count += 1;
        }
        lines.truncate(count);
        debug_assert_eq!(lines.len(), count);
        Ok(())
    }
}

/// Writes `text` over `current`, in its allocation when it fits.
fn rewrite(current: &mut String, text: &str) {
    if current != text {
        current.clear();
        current.push_str(text);
    }
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

    /// Three lines as liqctld's Python `json.dumps` writes them, degree sign escaped.
    const STATUS_BODY: &str = r#"{"status": [["Liquid temperature", "31.5", "\u00b0C"],
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

    fn parsed_into(body: &str, mut lines: LCStatus) -> serde_json::Result<LCStatus> {
        parse_status_into(body.as_bytes(), &mut lines)?;
        Ok(lines)
    }

    /// Goal: parsing into recycled lines gives what an owned parse gives, whatever the lines
    /// held before: fewer, more, or none, and with escaped text decoded. Method: parse one
    /// reply into each kind of recycled list and compare with `from_str`.
    #[test]
    fn a_status_parsed_into_recycled_lines_matches_an_owned_parse() {
        let owned: StatusResponse = serde_json::from_str(STATUS_BODY).unwrap();
        let line = |name: &str| (name.to_string(), "1".to_string(), SkippedUnit);
        let escaped = r#"{"status": [["Fan \"1\"", "a\\b", "%"]], "other": [1]}"#;

        for recycled in [vec![], vec![line("old")], vec![line("a"); 5]] {
            assert_eq!(parsed_into(STATUS_BODY, recycled).unwrap(), owned.status);
        }
        let decoded = parsed_into(escaped, vec![line("old")]).unwrap();
        assert_eq!(
            decoded,
            vec![(r#"Fan "1""#.to_string(), r"a\b".to_string(), SkippedUnit)]
        );
    }

    /// Goal: a reply that is not a status fails as an owned parse would, rather than leaving a
    /// partial status that looks whole. Method: a reply with no status, one with two, one with
    /// trailing text, and a malformed line.
    #[test]
    fn a_reply_that_is_not_a_status_fails_to_parse() {
        let bodies = [
            r#"{"devices": []}"#,
            r#"{"status": [], "status": []}"#,
            r#"{"status": []} trailing"#,
            r#"{"status": [["Pump speed", "2000"]]}"#,
        ];

        for body in bodies {
            assert!(parsed_into(body, Vec::new()).is_err(), "{body}");
            assert!(
                serde_json::from_str::<StatusResponse>(body).is_err(),
                "{body}"
            );
        }
    }

    /// Goal: once warm, a status line costs nothing to parse: names and values are written into
    /// last tick's buffers. Method: parse a reply into recycled lines, then count a second parse.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_status_parse_allocates_nothing() {
        let mut lines = parsed_into(STATUS_BODY, Vec::new()).unwrap();

        let (result, allocations) = crate::alloc_count::count_allocations(|| {
            parse_status_into(STATUS_BODY.as_bytes(), &mut lines)
        });

        result.unwrap();
        assert_eq!(allocations, 0);
        assert_eq!(lines.len(), 3);
    }
}
