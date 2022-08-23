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


use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::identity;
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use flume::{Receiver, Sender};
use log::{debug, error, warn};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{Client, Device};
use crate::device::{DeviceType, Status};
use crate::liquidctl::base_driver::BaseDriver;
use crate::liquidctl::device_mapper::DeviceMapper;
use crate::repository::Repository;
use crate::setting::Setting;

pub struct LiquidctlRepo {
    client_thread: JoinHandle<()>,
    tx_to_client: Sender<ClientMessage>,
    rx_from_client: Receiver<String>,
    device_mapper: DeviceMapper,
    devices: RefCell<Vec<Device>>,
}

type LCStatus = Vec<(String, String, String)>;
type StatusMap = HashMap<String, String>;

impl LiquidctlRepo {
    pub fn new() -> Result<Self> {
        let (
            tx_from_repo_to_client,
            rx_from_repo_to_client
        ) = flume::unbounded();
        let (
            tx_from_client_to_repo,
            rx_from_client_to_repo
        ) = flume::unbounded();
        let client = match LiquidctlRepo::connect_liqctld() {
            Ok(client) => client,
            Err(err) => bail!("{}", err)
        };
        let client_thread = thread::spawn(
            move || LiquidctlRepo::client_engine(tx_from_client_to_repo,
                                                 rx_from_repo_to_client,
                                                 client)
        );
        Ok(LiquidctlRepo {
            client_thread,
            tx_to_client: tx_from_repo_to_client,
            rx_from_client: rx_from_client_to_repo,
            device_mapper: DeviceMapper::new(),
            devices: RefCell::new(vec![]),
        })
    }

    fn connect_liqctld() -> Result<Client> {
        let mut retry_count: u8 = 0;
        while retry_count < 5 {
            match Client::new() {
                Ok(client) => {
                    match client.handshake() {
                        Ok(()) => return Ok(client),
                        Err(err) => error!("Liqctld Handshake error: {}", err)
                    };
                }
                Err(err) =>
                    error!(
                    "Could not establish liqctld socket connection, retry #{}. \n{}",
                    retry_count, err
                )
            };
            sleep(Duration::from_secs(1));
            retry_count += 1;
        }
        bail!("Failed to connect to liqctld after {} retries", retry_count);
    }

    fn client_engine(tx_to_repo: Sender<String>, rx_from_repo: Receiver<ClientMessage>, client: Client) {
        loop {
            let msg = match rx_from_repo.recv() {
                Ok(msg) => msg,
                Err(err) => {
                    error!("Error encountered waiting for message from Repo: {}", err);
                    Self::client_quit(&client);
                    break;
                }
            };

            match msg {
                ClientMessage::Quit => {
                    Self::client_quit(&client);
                    tx_to_repo
                        .send("Quit".to_string())
                        .map_err(|err| warn!("Error sending signal to client: {}", err))
                        .ok();
                    break;
                }
                ClientMessage::FindDevices => {
                    match client.find_devices() {
                        Ok(devices) => tx_to_repo.send(devices).ok(),
                        Err(err) => tx_to_repo.send(err.to_string()).ok()
                    };
                }
                ClientMessage::GetStatus(device_id) => {
                    match client.get_status(&device_id) {
                        Ok(status) => tx_to_repo.send(status).ok(),
                        Err(err) => tx_to_repo.send(err.to_string()).ok()
                    };
                }
                client_msg => warn!("Client Message logic not yet implemented: {:?}", client_msg)
            };
        }
    }

    fn client_quit(client: &Client) {
        client.quit()
            .unwrap_or_else(|err| error!("Error shutting down Liquidctl Repo: {}", err))
    }

    fn map_device_type(&self, device: &DeviceListResponse) -> Option<BaseDriver> {
        serde_json::from_str(format!("\"{}\"", device.device_type).as_str())
            .ok()
            .filter(|driver| self.device_mapper.is_device_supported(driver))
    }

    fn map_status(&self,
                  device_type: &BaseDriver,
                  lc_statuses: &LCStatus,
                  device_id: &u8,
    ) -> Status {
        let mut status_map: HashMap<String, String> = HashMap::new();
        for lc_status in lc_statuses {
            status_map.insert(lc_status.0.to_lowercase(), lc_status.1.clone());
        }
        self.device_mapper.extract_status(device_type, &status_map, device_id)
    }

    fn get_status(&self,
                  device_type: &BaseDriver,
                  device_id: &u8,
    ) -> Option<Status> {
        self.tx_to_client
            .send(ClientMessage::GetStatus(device_id.clone()))
            .unwrap_or_else(|err| error!("Error sending signal to client thread: {}", err));
        match self.rx_from_client.recv_timeout(Duration::from_secs(3)) {
            Ok(status_str) => {
                serde_json::from_str::<LCStatus>(status_str.as_str())
                    .map_or_else(
                        |err| {
                            error!("Could not deserialize response: {:?}", err);
                            Some(Status { ..Default::default() })
                        },
                        |lc_statuses| Some(self.map_status(device_type, &lc_statuses, device_id)),
                    )
            }
            Err(err) => {
                warn!("Error waiting on response from client: {}", err);
                None
            }
        }
    }
}

impl Repository for LiquidctlRepo {
    fn initialize_devices(&self) {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.tx_to_client
            .send(ClientMessage::FindDevices)
            .map_err(|err| error!("Error sending signal to client thread: {}", err))
            .ok();
        match self.rx_from_client.recv_timeout(Duration::from_secs(20)) {
            Ok(device_list_str) => {
                debug!(
                    "Time taken to initialize all liquidctl devices: {:?}",
                    start_initialization.elapsed());
                let device_list: Vec<DeviceListResponse> =
                    serde_json::from_str(device_list_str.as_str())
                        .map_or_else(
                            |err| {
                                error!("Could not deserialize response: {:?}", err);
                                vec![]
                            },
                            identity,
                        );
                debug!("Received Device List: {:?}", device_list);
                for device in device_list {
                    let device_type = match self.map_device_type(&device) {
                        None => {
                            warn!("Device is currently not supported: {:?}", device.device_type);
                            continue;
                        }
                        Some(d_type) => d_type
                    };
                    let init_status = self.map_status(
                        &device_type, &device.status, &device.id,
                    );
                    let firmware_version = init_status.firmware_version.clone();
                    let mut statuses = vec![init_status];
                    let status = self.get_status(&device_type, &device.id);
                    if status.is_some() {
                        statuses.push(status.unwrap())
                    }
                    self.devices.borrow_mut().push(
                        Device {
                            name: device.description,
                            d_type: DeviceType::Liquidctl,
                            type_id: device.id,
                            status_history: RefCell::new(statuses),
                            colors: Default::default(),
                            lc_driver_type: Some(device_type),
                            lc_init_firmware_version: firmware_version,
                            info: None,  // todo:
                        }
                    );
                }
                debug!("Initialized Devices: {:?}", self.devices);
            }
            Err(err) => warn!("Error waiting on response from client: {}", err)
        }
    }

    fn devices(&self) {
        todo!()
    }

    fn update_statuses(&self) {
        for device in self.devices.borrow().iter() {
            let status_opt = self.get_status(
                &device.lc_driver_type.clone().unwrap(), &device.type_id,
            );
            if let Some(status) = status_opt {
                device.set_status(status);
            }
        }
    }

    fn shutdown(&self) {
        debug!("Shutting down Liquidctl Repo");
        self.tx_to_client
            .send(ClientMessage::Quit)
            .unwrap_or_else(
                |err| error!("Error sending quit signal to client thread: {}", err)
            );
        match self.rx_from_client.recv_timeout(Duration::from_secs(2)) {
            Ok(_) => {}
            Err(err) => warn!("Error waiting on Quit response from client: {}", err)
        }
    }

    fn apply_setting(&self, device_type_id: u8, setting: Setting) {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum ClientMessage {
    FindDevices,
    GetStatus(u8),
    SetSpeed(String),
    SetLighting(String),
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceListResponse {
    id: u8,
    description: String,
    status: LCStatus,
    device_type: String,
}