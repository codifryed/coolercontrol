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
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use const_format::concatcp;
use log::{debug, error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::sleep;
use zbus::export::futures_util::future::join_all;

use crate::Device;
use crate::device::{DeviceType, Status};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::device_mapper::DeviceMapper;
use crate::repositories::liquidctl::liqctld_client::LiqctldUpdateClient;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

pub const LIQCTLD_ADDRESS: &str = "http://127.0.0.1:11986";
const LIQCTLD_HANDSHAKE: &str = concatcp!(LIQCTLD_ADDRESS, "/handshake");
const LIQCTLD_DEVICES: &str = concatcp!(LIQCTLD_ADDRESS, "/devices");
const LIQCTLD_DEVICES_CONNECT: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/connect");
const LIQCTLD_LEGACY690: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/legacy690");
const LIQCTLD_INITIALIZE: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/initialize");
const LIQCTLD_QUIT: &str = concatcp!(LIQCTLD_ADDRESS, "/quit");

type LCStatus = Vec<(String, String, String)>;

pub struct LiquidctlRepo {
    client: Client,
    device_mapper: DeviceMapper,
    devices: HashMap<u8, DeviceLock>,
    pub liqctld_update_client: Arc<LiqctldUpdateClient>,
}

impl LiquidctlRepo {
    pub async fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        // todo: self generated certs
        Self::establish_connection(&client).await?;
        info!("Communication established with Liqctld.");
        let liqctld_update_client = LiqctldUpdateClient::new(client.clone()).await?;
        Ok(LiquidctlRepo {
            client,
            device_mapper: DeviceMapper::new(),
            devices: HashMap::new(),
            liqctld_update_client: Arc::new(liqctld_update_client),
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
            self.liqctld_update_client.create_update_queue(&device_response.id).await;
            self.devices.insert(
                device_response.id,
                Arc::new(RwLock::new(Device {
                    name: device_response.description,
                    d_type: DeviceType::Liquidctl,
                    type_id: device_response.id,
                    lc_driver_type: Some(device_type),
                    info: None,  // todo
                    ..Default::default()
                })),
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
        let mut futures = vec![];
        for device in self.devices.values() {
            futures.push(self.call_initialize_per_device(device));
        }
        let results: Vec<Result<()>> = join_all(futures).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(err) => error!("Error getting initializing device: {}", err)
            }
        }
    }

    async fn call_initialize_per_device(&self, device_lock: &DeviceLock) -> Result<()> {
        let mut device = device_lock.write().await;
        let status_response = self.client.borrow()
            .post(LIQCTLD_INITIALIZE
                .replace("{}", device.type_id.to_string().as_str())
            )
            .json(&InitializeRequest { pump_mode: None })
            .send().await?
            .json::<StatusResponse>().await?;
        let init_status = self.map_status(
            device.lc_driver_type.as_ref().expect("This should always be set for liquidctl devices"),
            &status_response.status,
            &device.type_id,
        );
        device.lc_init_firmware_version = init_status.firmware_version.clone();
        device.set_status(init_status);
        Ok(())
    }
}

#[async_trait]
impl Repository for LiquidctlRepo {
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.call_initialize_concurrently().await;
        let mut init_devices = vec![];
        for device in self.devices.values() {
            init_devices.push(device.read().await.clone())
        }
        debug!("Initialized Devices: {:?}", init_devices);
        debug!(
            "Time taken to initialize all liquidctl devices: {:?}", start_initialization.elapsed()
        );
        info!("All liquidctl devices initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    /// This works differently than by other repositories, because we preload the status in a
    /// liqctld_update_client queue so we don't lock the repositories for long periods of time.
    /// This keeps the response time for UI Device Status calls nice and low.
    async fn update_statuses(&self) -> Result<()> {
        for deviceL_lock in self.devices.values() {
            let mut device = deviceL_lock.write().await;
            let lc_status = self.liqctld_update_client
                .get_update_for_device(&device.type_id).await;
            if let Err(err) = lc_status {
                error!("{}", err);
                continue;
            }
            let status = self.map_status(
                device.lc_driver_type.as_ref().unwrap(),
                &lc_status.unwrap(),
                &device.type_id,
            );
            device.set_status(status)
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        debug!("Shutting down Liquidctl Repo");
        let quit_response = self.client
            .post(LIQCTLD_QUIT)
            .send().await?
            .json::<QuitResponse>().await?;
        debug!("Liquidctl Repo Shutdown");
        return if quit_response.quit {
            info!("Quit Signal successfully sent to Liqctld");
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
pub struct StatusResponse {
    pub status: LCStatus,
}

