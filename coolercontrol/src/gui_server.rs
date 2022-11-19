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

use std::ops::Deref;
use actix_web::{App, get, HttpServer, middleware, post, Responder, web};
use actix_web::dev::Server;
use actix_web::web::{Data, Json};
use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Device, Repos};
use crate::device::{DeviceInfo, DeviceType, Status};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::repository::DeviceLock;

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";

/// Returns a simple handshake to verify established connection
#[get("/handshake")]
async fn handshake() -> impl Responder {
    web::Json(json!({"shake": true}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceDto {
    pub name: String,
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_id: u8,
    pub uid: String,
    pub lc_driver_type: Option<BaseDriver>,
    pub lc_init_firmware_version: Option<String>,
    pub info: Option<DeviceInfo>,
}

impl From<&Device> for DeviceDto {
    fn from(device: &Device) -> Self {
        Self {
            name: device.name.clone(),
            d_type: device.d_type.clone(),
            type_id: device.type_index,
            uid: device.uid.clone(),
            lc_driver_type: device.lc_driver_type.clone(),
            lc_init_firmware_version: device.lc_firmware_version.clone(),
            info: device.info.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<DeviceDto>,
}

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
#[get("/devices")]
async fn devices(repos: Data<Repos>) -> impl Responder {
    let mut all_devices = vec![];
    for repo in repos.iter() {
        for device_lock in repo.devices().await {
            all_devices.push(device_lock.read().await.deref().into())
        }
    }
    web::Json(DevicesResponse { devices: all_devices })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusRequest {
    only_current: Option<bool>,
    since: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceStatusDto {
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: String,
    pub status_history: Vec<Status>,
}

impl From<&Device> for DeviceStatusDto {
    fn from(device: &Device) -> Self {
        Self {
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            status_history: device.status_history.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusResponse {
    devices: Vec<DeviceStatusDto>,
}

/// Returns the status of all devices with the selected filters from the request body
#[post("/status")]
async fn status(status_request: web::Json<StatusRequest>, repos: Data<Repos>) -> impl Responder {
    let mut all_devices = vec![];
    for repo in repos.iter() {
        for device_lock in repo.devices().await {
            let dto = transform_status(&status_request, &device_lock).await;
            all_devices.push(dto);
        }
    }
    Json(StatusResponse { devices: all_devices })
}

async fn transform_status(status_request: &Json<StatusRequest>, device_lock: &DeviceLock) -> DeviceStatusDto {
    let device = device_lock.read().await;
    if let Some(true) = status_request.only_current {
        if let Some(last_status) = device.status_history.last() {
            return DeviceStatusDto {
                d_type: device.d_type.clone(),
                type_index: device.type_index,
                uid: device.uid.clone(),
                status_history: vec![last_status.clone()],
            };
        }
    } else if let Some(since_timestamp) = status_request.since {
        let filtered_history = device.status_history.iter()
            .filter(|device_status| device_status.timestamp >= since_timestamp)
            .map(|device_status| device_status.clone())
            .collect();
        return DeviceStatusDto {
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            status_history: filtered_history,
        };
    };
    device.deref().into()
}

pub async fn init_server(repos: Repos) -> Result<Server> {
    let server = HttpServer::new(move || {
        App::new()
            // todo: if log::max_level() == LevelFilter::Debug set app logger, otherwise no
            .wrap(middleware::Logger::default())
            // todo: cors?
            // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
            .app_data(Data::new(repos.clone()))
            .service(handshake)
            .service(devices)
            .service(status)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
