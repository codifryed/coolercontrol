/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use std::ops::Deref;

use actix_web::web::{Data, Json};
use actix_web::{post, Responder};
use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLockReadGuard;

use crate::device::{DeviceType, Status, UID};
use crate::repositories::repository::DeviceLock;
use crate::{AllDevices, Device};

lazy_static! {
    // possible scheduled update variance (<100ms) + all devices updated avg timespan (~80ms)
    pub static ref MAX_UPDATE_TIMESTAMP_VARIATION: chrono::Duration = chrono::Duration::milliseconds(200);
}

/// Returns the status of all devices with the selected filters from the request body
#[post("/status")]
async fn get_status(
    status_request: Json<StatusRequest>,
    all_devices: Data<AllDevices>,
) -> impl Responder {
    let mut all_devices_list = vec![];
    for device_lock in all_devices.values() {
        let dto = transform_status(&status_request, device_lock).await;
        all_devices_list.push(dto);
    }
    Json(StatusResponse {
        devices: all_devices_list,
    })
}

async fn transform_status(
    status_request: &Json<StatusRequest>,
    device_lock: &DeviceLock,
) -> DeviceStatusDto {
    let device = device_lock.read().await;
    if let Some(true) = status_request.all {
        get_all_statuses(&device)
    } else if let Some(since_timestamp) = status_request.since {
        get_statuses_since(since_timestamp, &device)
    } else {
        get_most_recent_status(device)
    }
}

fn get_all_statuses(device: &RwLockReadGuard<Device>) -> DeviceStatusDto {
    device.deref().into()
}

fn get_statuses_since(
    since_timestamp: DateTime<Local>,
    device: &RwLockReadGuard<Device>,
) -> DeviceStatusDto {
    let timestamp_limit = since_timestamp + *MAX_UPDATE_TIMESTAMP_VARIATION;
    let filtered_history = device
        .status_history
        .iter()
        .filter(|device_status| device_status.timestamp > timestamp_limit)
        .cloned()
        .collect();
    DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history: filtered_history,
    }
}

fn get_most_recent_status(device: RwLockReadGuard<Device>) -> DeviceStatusDto {
    let mut status_history: Vec<Status> = Vec::with_capacity(1);
    if let Some(most_recent_status) = device.status_current() {
        status_history.push(most_recent_status);
    }
    DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history,
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
