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


use std::borrow::{Borrow};
use std::clone::Clone;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use std::string::ToString;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use const_format::concatcp;
use log::{debug, error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::sleep;
use zbus::export::futures_util::future;

use crate::Device;
use crate::device::{DeviceType, Status};
use crate::liquidctl::base_driver::BaseDriver;
use crate::liquidctl::device_mapper::DeviceMapper;
use crate::repository::Repository;
use crate::setting::Setting;

pub struct LiquidctlRepo {
    client: Client,
    device_mapper: DeviceMapper,
    devices: RwLock<Vec<Device>>,
}

type LCStatus = Vec<(String, String, String)>;

const LIQCTLD_ADDRESS: &str = "http://127.0.0.1:11986";
const LIQCTLD_HANDSHAKE: &str = concatcp!(LIQCTLD_ADDRESS, "/handshake");
const LIQCTLD_DEVICES: &str = concatcp!(LIQCTLD_ADDRESS, "/devices");
const LIQCTLD_DEVICES_CONNECT: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/connect");
const LIQCTLD_LEGACY690: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/legacy690");
const LIQCTLD_INITIALIZE: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/initialize");
const LIQCTLD_STATUS: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/status");
const LIQCTLD_QUIT: &str = concatcp!(LIQCTLD_ADDRESS, "/quit");


impl LiquidctlRepo {
    pub async fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        // todo: self generated certs
        LiquidctlRepo::establish_connection(&client).await?;
        info!("Communication established with Liqctld.");
        Ok(LiquidctlRepo {
            client,
            device_mapper: DeviceMapper::new(),
            devices: RwLock::new(vec![]),
        })
    }

    async fn establish_connection(client: &Client) -> Result<()> {
        let mut retry_count: u8 = 0;
        while retry_count < 5 {
            match client.get(LIQCTLD_HANDSHAKE).send().await {
                Ok(response) =>
                    return match response.json::<HandshakeResponse>().await {
                        Ok(handshake_response) => if handshake_response.shake {
                            Ok(())
                        } else {
                            Err(anyhow!(
                                    "Incorrect Handshake confirmation. Shake: {}",
                                    handshake_response.shake)
                            )
                        }
                        Err(err) => Err(anyhow!(err))
                    },
                Err(err) =>
                    error!(
                    "Could not establish communication with liqctld socket connection, retry #{}. \n{}",
                    retry_count + 1, err
                )
            };
            sleep(Duration::from_secs(1)).await;
            retry_count += 1;
        }
        bail!("Failed to connect to liqctld after {} tries", retry_count);
    }

    pub async fn get_devices(&mut self) -> Result<()> {
        let devices_response = self.client.get(LIQCTLD_DEVICES)
            .send().await?
            .json::<DevicesResponse>().await?;
        for device_response in devices_response.devices {
            let device_type = match self.map_device_type(&device_response) {
                None => {
                    warn!("Device is currently not supported: {:?}", device_response.device_type);
                    continue;
                }
                Some(d_type) => d_type
            };
            // self.devices.borrow_mut().push(
            self.devices.get_mut().push(
                Device {
                    name: device_response.description,
                    d_type: DeviceType::Liquidctl,
                    type_id: device_response.id,
                    status_history: vec![],
                    lc_driver_type: Some(device_type),
                    lc_init_firmware_version: None,
                    info: None,
                }
            );
        }
        debug!("List of received Devices: {:?}", self.devices);
        Ok(())
    }

    pub async fn connect_devices(&self) -> Result<()> {
        let connection_response = self.client.post(LIQCTLD_DEVICES_CONNECT)
            .send().await?
            .json::<ConnectionResponse>().await?;
        if connection_response.connected {
            info!("All Liquidctl Devices connected");
            Ok(())
        } else {
            Err(anyhow!("Incorrect Connect Devices Response: {}", connection_response.connected))
        }
    }

    fn map_device_type(&self, device: &DeviceResponse) -> Option<BaseDriver> {
        BaseDriver::from_str(device.device_type.as_str())
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

    async fn call_initialize_concurrently(&self) {
        let mut devices_borrowed = self.devices.write().await;
        let mut futures = vec![];
        for device in devices_borrowed.deref_mut() {
            futures.push(self.call_initialize_per_device(device));
        }
        let results: Vec<Result<()>> = future::join_all(futures).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(err) => error!("Error getting initializing device: {}", err)
            }
        }
    }

    async fn call_initialize_per_device(&self, device: &mut Device) -> Result<()> {
        let status_response = self.client.borrow()
            .post(LIQCTLD_INITIALIZE.replace("{}", device.type_id.to_string().as_str()))
            .json(&InitializeRequest { pump_mode: None })
            .send().await?
            .json::<StatusResponse>().await?;
        let init_status = self.map_status(
            device.lc_driver_type.as_ref().unwrap(),
            &status_response.status,
            &device.type_id,
        );
        device.lc_init_firmware_version = init_status.firmware_version.clone();
        device.set_status(init_status);
        Ok(())
    }

    async fn call_status_concurrently(&self) {
        let mut devices_borrowed = self.devices.write().await;
        let mut futures = vec![];
        for device in devices_borrowed.deref_mut() {
            futures.push(self.call_status_per_device(device));
        }
        let results: Vec<Result<()>> = future::join_all(futures).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(err) => error!("Error getting status from device: {}", err)
            }
        }
    }

    async fn call_status_per_device(&self, device: &mut Device) -> Result<()> {
        let status_response = self.client.borrow()
            .get(LIQCTLD_STATUS.replace("{}", device.type_id.to_string().as_str()))
            .send().await?  // todo: will .with_context help here to determine the device it has issues with?
            .json::<StatusResponse>().await?;
        let init_status = self.map_status(
            device.lc_driver_type.as_ref().unwrap(),
            &status_response.status,
            &device.type_id,
        );
        debug!("Status updated for LC Device #{} with: {:?}", &device.type_id, &init_status);
        device.set_status(init_status);
        Ok(())
    }
}

#[async_trait]
impl Repository for LiquidctlRepo {
    async fn initialize_devices(&self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.call_initialize_concurrently().await;
        debug!("Initialized Devices: {:?}", self.devices.read().await);
        debug!(
            "Time taken to initialize all liquidctl devices: {:?}", start_initialization.elapsed()
        );
        info!("All liquidctl devices initialized");
        Ok(())
    }

    async fn devices(&self) -> Vec<Device> {
        let mut vec = vec![];
        for dev in self.devices.read().await.deref() {
            vec.push(dev.clone())  // Currently clones all devices
        }
        vec
    }

    async fn update_statuses(&self) -> Result<()> {
        debug!("Updating all Liquidctl device statuses");
        let start_initialization = Instant::now();
        self.call_status_concurrently().await;
        debug!(
            "Time taken to get status for all liquidctl devices: {:?}",
            start_initialization.elapsed()
        );
        info!("All liquidctl device statuses updated");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        debug!("Shutting down Liquidctl Repo");
        let quit_response = self.client
            .post(LIQCTLD_QUIT)
            .send().await?
            .json::<QuitResponse>().await?;
        return if quit_response.quit {
            info!("Quit successfully sent to Liqctld");
            Ok(())
        } else {
            Err(anyhow!("Incorrect quit response from liqctld: {}", quit_response.quit))
        };
    }

    async fn apply_setting(&self, device_type_id: u8, setting: Setting) -> Result<()> {
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandshakeResponse {
    shake: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuitResponse {
    quit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceResponse {
    id: u8,
    description: String,
    device_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<DeviceResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConnectionResponse {
    connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitializeRequest {
    pump_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusResponse {
    status: LCStatus,
}

