/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::os::linux::raw::stat;

use heck::ToTitleCase;
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;

use crate::device::{DeviceInfo, LightingMode, Status, TempStatus};

type StatusMap = HashMap<String, String>;

fn parse_float(value: &String) -> Option<f64> {
    value.parse::<f64>().ok()
}

/// It is a general purpose trait and each supported device struc must implement this trait.
/// Many of the default methods will cover all use cases and it is advisable to override them
/// for increase efficiency and performance.
pub trait DeviceSupport: Debug {
    // todo: device_instance won't be available for this method,
    //  we may need to send extra data from liqctld for this
    fn extract_info(&self) -> DeviceInfo;

    // todo: in python this is an *args: Any parameter... that won't fly here.
    fn get_filtered_color_channel_modes(&self) -> Vec<LightingMode>;

    fn extract_status(&self, status_map: &StatusMap, device_id: &u8) -> Status {
        Status {
            firmware_version: self.get_firmware_ver(status_map),
            temps: self.get_temperatures(status_map, device_id),
            // todo:
            // channels: vec![]
            ..Default::default()
        }
    }

    fn get_firmware_ver(&self, status_map: &StatusMap) -> Option<String> {
        status_map.get("firmware version").cloned()
    }


    /// It's possible to override this method and use only the needed sub-functions per device
    fn get_temperatures(&self,
                        status_map: &StatusMap,
                        device_id: &u8,
    ) -> Vec<TempStatus> {
        let mut temps = vec![];
        self.add_liquid_temp(status_map, &mut temps, device_id);
        self.add_water_temp(status_map, &mut temps, device_id);
        self.add_temp(status_map, &mut temps, device_id);
        self.add_temp_probes(status_map, &mut temps, device_id);
        self.add_noise_level(status_map, &mut temps, device_id);
        temps
    }

    fn add_liquid_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>, device_id: &u8) {
        let liquid_temp = status_map.get("liquid temperature")
            .and_then(parse_float);
        if let Some(temp) = liquid_temp {
            temps.push(TempStatus {
                name: "liquid".to_string(),
                temp,
                frontend_name: "Liquid".to_string(),
                external_name: format!("LC#{} Liquid", device_id),
            })
        }
    }

    fn add_water_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>, device_id: &u8) {
        let water_temp = status_map.get("water temperature")
            .and_then(parse_float);
        if let Some(temp) = water_temp {
            temps.push(TempStatus {
                name: "water".to_string(),
                temp,
                frontend_name: "Water".to_string(),
                external_name: format!("LC#{} Water", device_id),
            })
        }
    }

    fn add_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>, device_id: &u8) {
        let plain_temp = status_map.get("temperature")
            .and_then(parse_float);
        if let Some(temp) = plain_temp {
            temps.push(TempStatus {
                name: "temp".to_string(),
                temp,
                frontend_name: "Temp".to_string(),
                external_name: format!("LC#{} Temp", device_id),
            })
        }
    }

    fn add_temp_probes(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>, device_id: &u8) {
        lazy_static!(
            static ref TEMP_PROB_PATTERN: Regex = Regex::new(r"temperature \d+").unwrap();
            static ref NUMBER_PATTERN: Regex = Regex::new(r"\d+").unwrap();
        );
        for (probe_name, value) in status_map.iter() {
            if TEMP_PROB_PATTERN.is_match(probe_name) {
                if let Some(temp) = parse_float(value) {
                    if let Some(probe_number) = NUMBER_PATTERN.find_at(probe_name, probe_name.len() - 2) {
                        let name = format!("temp{}", probe_number.as_str());
                        temps.push(TempStatus {
                            temp,
                            frontend_name: name.to_title_case(),
                            external_name: format!("LC#{} {}", device_id, name.to_title_case()),
                            name,
                        })
                    }
                }
            }
        }
    }

    fn add_noise_level(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>, device_id: &u8) {
        let noise_lvl = status_map.get("noise level")
            .and_then(parse_float);
        if let Some(noise) = noise_lvl {
            temps.push(TempStatus {
                name: "noise".to_string(),
                temp: noise,
                frontend_name: "Noise dB".to_string(),
                external_name: format!("LC#{} Noise dB", device_id),
            })
        }
    }
}

/// Support for the Liquidctl KrakenX3 Driver
#[derive(Debug)]
pub struct KrakenX3Support;

impl KrakenX3Support {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for KrakenX3Support {
    fn extract_info(&self) -> DeviceInfo {
        todo!()
    }

    fn get_filtered_color_channel_modes(&self) -> Vec<LightingMode> {
        todo!()
    }
}