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

use std::sync::Arc;
use actix_web::{App, HttpServer, middleware, Responder, web, get, HttpRequest};
use actix_web::dev::Server;
use actix_web::web::Data;
use serde::{Deserialize, Serialize};

use anyhow::Result;
use serde_json::json;
use tokio::sync::RwLock;

use crate::{Device, Repos, Repository};

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

#[get("/handshake")]
async fn handshake() -> impl Responder {
    web::Json(json!({"shake": true}))
}

#[get("/devices")]
async fn devices(repos: Data<Repos>) -> impl Responder {
    let mut all_devices: Vec<Device> = vec![];
    for repo in repos.read().await.iter() {
        all_devices.extend(repo.devices().await)
    }
    web::Json(DevicesResponse { devices: all_devices })
}

pub async fn init_server(repos: Repos) -> Result<Server> {
    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            // todo: cors?
            .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
            .app_data(Data::new(repos.clone()))
            .service(handshake)
            .service(devices)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
