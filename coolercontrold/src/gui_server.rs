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
use std::sync::Arc;

use actix_web::{App, get, HttpResponse, HttpServer, middleware, patch, post, Responder};
use actix_web::dev::Server;
use actix_web::web::{Data, Json, Path};
use anyhow::Result;
use chrono::{DateTime, Local};
use log::error;
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{AllDevices, Device};
use crate::config::Config;
use crate::device::{DeviceInfo, DeviceType, LcInfo, Status, UID};
use crate::device_commander::DeviceCommander;
use crate::repositories::repository::DeviceLock;
use crate::setting::Setting;

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Returns a simple handshake to verify established connection
#[get("/handshake")]
async fn handshake() -> impl Responder {
    Json(json!({"shake": true}))
}

#[post("/shutdown")]
async fn shutdown() -> impl Responder {
    signal::kill(Pid::this(), Signal::SIGQUIT).unwrap();
    Json(json!({"shutdown": true}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceDto {
    pub name: String,
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub lc_info: Option<LcInfo>,
    pub info: Option<DeviceInfo>,
}

impl From<&Device> for DeviceDto {
    fn from(device: &Device) -> Self {
        Self {
            name: device.name.clone(),
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            lc_info: device.lc_info.clone(),
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
async fn devices(all_devices: Data<AllDevices>) -> impl Responder {
    let mut all_devices_list = vec![];
    for device_lock in all_devices.values() {
        all_devices_list.push(device_lock.read().await.deref().into())
    }
    Json(DevicesResponse { devices: all_devices_list })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusRequest {
    all: Option<bool>,
    since: Option<DateTime<Local>>,
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
async fn status(status_request: Json<StatusRequest>, all_devices: Data<AllDevices>) -> impl Responder {
    let mut all_devices_list = vec![];
    for device_lock in all_devices.values() {
        let dto = transform_status(&status_request, &device_lock).await;
        all_devices_list.push(dto);
    }
    //    Let's do these in the UI actually:
    //    reasonable_hwmon_filter: bool
    //    hwmon_temps: bool
    //    thinkpad_hwmon_temps: bool
    //    only_composite_temps: bool  // This is for m2 hard drives that report Composite temperatures (the others are really needed)
    Json(StatusResponse { devices: all_devices_list })
}

async fn transform_status(status_request: &Json<StatusRequest>, device_lock: &DeviceLock) -> DeviceStatusDto {
    let device = device_lock.read().await;
    if let Some(true) = status_request.all {
        return device.deref().into();
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
    let status_history = if let Some(last_status) = device.status_current() {
        vec![last_status]
    } else { vec![] };
    DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history,
    }
}

/// Apply the settings sent in the request body to the associated device
#[patch("/devices/{device_uid}/setting")]
async fn settings(
    device_uid: Path<String>,
    settings_request: Json<Setting>,
    device_commander: Data<Arc<DeviceCommander>>,
    config: Data<Arc<Config>>
) -> impl Responder {
    match device_commander.set_setting(&device_uid.to_string(), settings_request.deref()).await {
        Ok(_) => {
            config.set_setting(&device_uid.to_string(), settings_request.deref()).await;
            if let Err(err) = config.save_config_file().await {
                error!("Error saving settings to config file: {}", err)
            }
            HttpResponse::Ok().json(json!({"success": true}))
        },
        Err(err) => HttpResponse::InternalServerError()
            .json(Json(ErrorResponse { error: err.to_string() }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AseTek690Request {
    is_legacy690: bool,
}

/// Set AseTek Cooler driver type
/// This is needed to set Legacy690Lc or Modern690Lc device driver type
#[patch("/devices/{device_id}/asetek690")]
async fn asetek(
    device_uid: Path<String>, asetek690_request: Json<AseTek690Request>, config: Data<Arc<Config>>,
) -> impl Responder {
    config.set_legacy690_id(&device_uid.to_string(), &asetek690_request.is_legacy690).await;
    match config.save_config_file().await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

pub async fn init_server(all_devices: AllDevices, device_commander: Arc<DeviceCommander>, config: Arc<Config>) -> Result<Server> {
    let server = HttpServer::new(move || {
        App::new()
            // todo: if log::max_level() == LevelFilter::Debug set app logger, otherwise no
            .wrap(middleware::Logger::default())
            // todo: cors?
            // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
            .app_data(Data::new(all_devices.clone()))
            .app_data(Data::new(device_commander.clone()))
            .app_data(Data::new(config.clone()))
            .service(handshake)
            .service(shutdown)
            .service(devices)
            .service(status)
            .service(settings)
            .service(asetek)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
