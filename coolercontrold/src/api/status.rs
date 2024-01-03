/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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
 */

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use actix_web::{post, Responder};
use actix_web::web::{Data, Json};
use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLockReadGuard;

use crate::{AllDevices, Device};
use crate::api::utils;
use crate::config::Config;
use crate::device::{DeviceType, Status, UID};
use crate::repositories::repository::DeviceLock;

lazy_static! {
    // possible scheduled update variance (<100ms) + all devices updated avg timespan (~80ms)
    pub static ref MAX_UPDATE_TIMESTAMP_VARIATION: chrono::Duration = chrono::Duration::milliseconds(200);
}

/// Returns the status of all devices with the selected filters from the request body
#[post("/status")]
async fn get_status(
    status_request: Json<StatusRequest>,
    all_devices: Data<AllDevices>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let mut all_devices_list = vec![];
    let smoothing_level = config.get_settings().await
        .map(|cc_settings| cc_settings.smoothing_level)
        .unwrap_or(0);
    for device_lock in all_devices.values() {
        let dto = transform_status(&status_request, &device_lock, smoothing_level).await;
        all_devices_list.push(dto);
    }
    Json(StatusResponse { devices: all_devices_list })
}

async fn transform_status(
    status_request: &Json<StatusRequest>,
    device_lock: &DeviceLock,
    smoothing_level: u8,
) -> DeviceStatusDto {
    let device = device_lock.read().await;
    if let Some(true) = status_request.all {
        get_all_statuses(&device, smoothing_level)
    } else if let Some(since_timestamp) = status_request.since {
        get_statuses_since(since_timestamp, &device, smoothing_level)
    } else {
        get_most_recent_status(device, smoothing_level)
    }
}

fn get_all_statuses(device: &RwLockReadGuard<Device>, smoothing_level: u8) -> DeviceStatusDto {
    let mut device_dto: DeviceStatusDto = device.deref().into();
    smooth_all_temps_and_loads(&mut device_dto, smoothing_level);
    device_dto
}

fn get_statuses_since(
    since_timestamp: DateTime<Local>,
    device: &RwLockReadGuard<Device>,
    smoothing_level: u8,
) -> DeviceStatusDto {
    let timestamp_limit = since_timestamp + *MAX_UPDATE_TIMESTAMP_VARIATION;
    let filtered_history = device.status_history.iter()
        .filter(|device_status| device_status.timestamp > timestamp_limit)
        .map(|device_status| device_status.clone())
        .collect();
    let mut device_dto = DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history: filtered_history,
    };
    smooth_all_temps_and_loads(&mut device_dto, smoothing_level);
    device_dto
}

fn get_most_recent_status(device: RwLockReadGuard<Device>, smoothing_level: u8) -> DeviceStatusDto {
    let sample_size = if smoothing_level == 0 { 1 } else { (smoothing_level * utils::SMA_WINDOW_SIZE) as usize };
    let mut status_history: Vec<Status> = device.status_history.iter().rev()
        .take(sample_size)  // get latest sample_size
        .map(|device_status| device_status.clone())
        .collect();
    status_history.reverse();
    let mut device_dto = DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history,
    };
    smooth_all_temps_and_loads(&mut device_dto, smoothing_level);
    device_dto.status_history = if let Some(most_recent_status) = device_dto.status_history.last() {
        vec![most_recent_status.to_owned()]
    } else { vec![] };
    device_dto
}

// DEPRECATED: The smoothing and DTO transformation is an expensive operation and should
//  be avoided if possible. The new UI no longer uses the smoothing function and will probably 
//  handle it itself should it be desired in the future.
fn smooth_all_temps_and_loads(device_dto: &mut DeviceStatusDto, smoothing_level: u8) {
    // cpu and gpu only have single loads, multiple temps are possible
    if (device_dto.d_type != DeviceType::CPU
        && device_dto.d_type != DeviceType::GPU
        && device_dto.d_type != DeviceType::Composite
    ) || smoothing_level == 0 {
        return;
    }
    let mut all_temps = HashMap::new();
    device_dto.status_history.iter()
        .flat_map(|d_status| d_status.temps.as_slice())
        .for_each(|temp_status|
            all_temps.entry(temp_status.name.to_owned())
                .or_insert_with(Vec::new)
                .push(temp_status.temp.to_owned())
        );
    let all_loads: Vec<f64> = device_dto.status_history.iter()
        .filter_map(|d_status|
            d_status.channels.iter()
                .find(|channel|
                    channel.name.to_lowercase().contains("load")
                ))
        .filter_map(|channel| channel.duty)
        .collect();
    all_temps.iter_mut().for_each(|(_, temps)|
        *temps = utils::all_values_from_simple_moving_average(temps.as_slice(), smoothing_level)
    );
    let smoothed_loads = utils::all_values_from_simple_moving_average(
        all_loads.as_slice(), smoothing_level,
    );
    for (i, d_status) in device_dto.status_history.iter_mut().enumerate() {
        d_status.temps.iter_mut()
            .for_each(|temp_status|
                temp_status.temp = all_temps
                    .get(&temp_status.name)
                    .expect("Temp list should be present")[i]
            );
        d_status.channels.iter_mut()
            .filter(|channel| channel.name.to_lowercase().contains("load"))
            .for_each(|load_status| load_status.duty = Some(smoothed_loads[i]))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusRequest {
    all: Option<bool>,
    since: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusResponse {
    devices: Vec<DeviceStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceStatusDto {
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub status_history: Vec<Status>,
}

impl From<&Device> for DeviceStatusDto {
    fn from(device: &Device) -> Self {
        Self {
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            status_history: device.status_history.clone().into(),
        }
    }
}
