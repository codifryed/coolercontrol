// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The JSON bodies liqctld exchanges. The calls that send them are in `liqctld_io`.
use serde::Deserialize;
use serde::Serialize;

/// A standard liquidctl status response (name, value, metric).
pub type LCStatus = Vec<(String, String, String)>;

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
