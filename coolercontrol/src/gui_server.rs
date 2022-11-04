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

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use anyhow::Result;
use log::info;
use serde_json::json;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::RwLock;
use warp::{Filter, Reply};

use crate::{Device, Repository};

const GUI_SERVER_PORT: u16 = 11987;

pub async fn init_server(
    repos: Arc<RwLock<Vec<Box<dyn Repository>>>>
) -> Result<impl Future<Output=()> + 'static> {

    let handshake = warp::path("handshake")
        .and(warp::get())
        .map(|| warp::reply::json(&json!({"shake": true})));

    let devices = warp::path("devices")
        .and(warp::get())
        // todo: with a query_param  -> smoothness_lvl=[1-4]
        //  this we can use to calculate the smoothness in the handler and re-attach the new
        //  statuses.
        .and(with_repos(repos))
        .and_then(handle_devices);
        // .and_then(move || handle_devices(repos.clone()));

    let cors = warp::cors()
        .allow_methods(vec!["GET", "POST", "PATCH"])
        .allow_any_origin()
        .build();
    let log = warp::log("gui_server");
    let routes = handshake
        .or(devices)
        .with(cors)
        .with(log);
    // todo: FD from SystemD set??? (add -d options & euid check)
    let mut sig_term = signal(SignalKind::terminate())?;
    let mut sig_int = signal(SignalKind::interrupt())?;
    let mut sig_quit = signal(SignalKind::quit())?;
    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], GUI_SERVER_PORT), async move {
            tokio::select! {
                _ = sig_term.recv() => {},
                _ = sig_int.recv() => {},
                _ = sig_quit.recv() => {},
            }
            info!("GUI Server shutting down.")
        });
    Ok(server)
}

fn with_repos(
    repos: Arc<RwLock<Vec<Box<dyn Repository>>>>
) -> impl Filter<Extract=(Arc<RwLock<Vec<Box<dyn Repository>>>>, ), Error=Infallible> + Clone {
    warp::any().map(move || repos.clone())
}

async fn handle_devices(
    repos: Arc<RwLock<Vec<Box<dyn Repository>>>>
) -> Result<impl Reply, Infallible> {
    let mut all_devices: Vec<Device> = vec![];
    for repo in repos.read().await.iter() {
        let devs = repo.devices().await;
        all_devices.extend(devs);
    }
    let response = DevicesResponse { devices: all_devices };
    Ok(warp::reply::json(&response))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}
