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
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{App, get, HttpResponse, HttpServer, middleware, patch, post, Responder};
use actix_web::dev::Server;
use actix_web::middleware::{Compat, Condition};
use actix_web::web::{Data, Json, Path};
use anyhow::Result;
use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use log::{error, LevelFilter};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLockReadGuard;

use crate::{AllDevices, Device, utils};
use crate::config::Config;
use crate::device::{DeviceInfo, DeviceType, LcInfo, Status, UID};
use crate::device_commander::DeviceCommander;
use crate::repositories::repository::DeviceLock;
use crate::setting::{CoolerControlSettings, Setting};

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";
lazy_static! {
    // possible scheduled update variance (<100ms) + all devices updated avg timespan (~80ms)
    pub static ref MAX_UPDATE_TIMESTAMP_VARIATION: chrono::Duration = chrono::Duration::milliseconds(200);
}

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
async fn status(
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

fn smooth_all_temps_and_loads(device_dto: &mut DeviceStatusDto, smoothing_level: u8) {
    // cpu and gpu only have single loads, multiple temps are possible
    if (device_dto.d_type != DeviceType::CPU && device_dto.d_type != DeviceType::GPU)
        || smoothing_level == 0 {
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
struct SettingsResponse {
    settings: Vec<Setting>,
}

/// Returns the currently applied settings for the given device
#[get("/devices/{device_uid}/settings")]
async fn get_device_settings(
    device_uid: Path<String>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.get_device_settings(device_uid.as_str()).await {
        Ok(settings) => HttpResponse::Ok()
            .json(Json(SettingsResponse { settings })),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Apply the settings sent in the request body to the associated device
#[patch("/devices/{device_uid}/settings")]
async fn apply_device_settings(
    device_uid: Path<String>,
    settings_request: Json<Setting>,
    device_commander: Data<Arc<DeviceCommander>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let result = match device_commander.set_setting(&device_uid.to_string(), settings_request.deref()).await {
        Ok(_) => {
            config.set_device_setting(&device_uid.to_string(), settings_request.deref()).await;
            config.save_config_file().await
        }
        Err(err) => Err(err)
    };
    match result {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
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
    device_uid: Path<String>,
    asetek690_request: Json<AseTek690Request>,
    config: Data<Arc<Config>>,
    all_devices: Data<AllDevices>,
) -> impl Responder {
    config.set_legacy690_id(&device_uid.to_string(), &asetek690_request.is_legacy690).await;
    match config.save_config_file().await {
        Ok(_) => {
            // Device is now known. Legacy690Lc devices still require a restart of the daemon.
            if let Some(device) = all_devices.get(&device_uid.to_string()) {
                if device.read().await.lc_info.is_some() {
                    device.write().await.lc_info.as_mut().unwrap().unknown_asetek = false
                }
            }
            HttpResponse::Ok().json(json!({"success": true}))
        }
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkPadFanControlRequest {
    enable: bool,
}

#[post("/thinkpad_fan_control")]
async fn thinkpad_fan_control(
    fan_control_request: Json<ThinkPadFanControlRequest>,
    device_commander: Data<Arc<DeviceCommander>>,
) -> impl Responder {
    match device_commander.thinkpad_fan_control(&fan_control_request.enable).await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoolerControlSettingsDto {
    apply_on_boot: Option<bool>,
    handle_dynamic_temps: Option<bool>,
    startup_delay: Option<u8>,
    smoothing_level: Option<u8>,
    thinkpad_full_speed: Option<bool>,
}

impl CoolerControlSettingsDto {
    fn merge(&self, current_settings: CoolerControlSettings) -> CoolerControlSettings {
        let apply_on_boot = if let Some(apply) = self.apply_on_boot {
            apply
        } else {
            current_settings.apply_on_boot
        };
        let handle_dynamic_temps = if let Some(should_handle) = self.handle_dynamic_temps {
            should_handle
        } else {
            current_settings.handle_dynamic_temps
        };
        let startup_delay = if let Some(delay) = self.startup_delay {
            Duration::from_secs(delay.max(0).min(10) as u64)
        } else {
            current_settings.startup_delay
        };
        let smoothing_level = if let Some(level) = self.smoothing_level {
            level
        } else {
            current_settings.smoothing_level
        };
        let thinkpad_full_speed = if let Some(full_speed) = self.thinkpad_full_speed {
            full_speed
        } else {
            current_settings.thinkpad_full_speed
        };
        CoolerControlSettings {
            apply_on_boot,
            no_init: current_settings.no_init,
            handle_dynamic_temps,
            startup_delay,
            smoothing_level,
            thinkpad_full_speed,
        }
    }
}

impl From<&CoolerControlSettings> for CoolerControlSettingsDto {
    fn from(settings: &CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            handle_dynamic_temps: Some(settings.handle_dynamic_temps),
            startup_delay: Some(settings.startup_delay.as_secs() as u8),
            smoothing_level: Some(settings.smoothing_level),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
        }
    }
}

/// Get CoolerControl settings
#[get("/settings")]
async fn get_cc_settings(
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.get_settings().await {
        Ok(settings) => HttpResponse::Ok()
            .json(Json(CoolerControlSettingsDto::from(&settings))),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Apply CoolerControl settings
#[patch("/settings")]
async fn apply_cc_settings(
    cc_settings_request: Json<CoolerControlSettingsDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let result = match config.get_settings().await {
        Ok(current_settings) => {
            let settings_to_set = cc_settings_request.merge(current_settings);
            config.set_settings(&settings_to_set).await;
            config.save_config_file().await
        }
        Err(err) => Err(err)
    };
    match result {
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
            .wrap(Condition::new(
                log::max_level() == LevelFilter::Debug,
                Compat::new(middleware::Logger::default()),
            ))
            // todo: cors?
            // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
            .app_data(Data::new(all_devices.clone()))
            .app_data(Data::new(device_commander.clone()))
            .app_data(Data::new(config.clone()))
            .service(handshake)
            .service(shutdown)
            .service(devices)
            .service(status)
            .service(get_device_settings)
            .service(apply_device_settings)
            .service(get_cc_settings)
            .service(apply_cc_settings)
            .service(asetek)
            .service(thinkpad_fan_control)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
