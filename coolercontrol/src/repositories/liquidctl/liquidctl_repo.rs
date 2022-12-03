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


use std::borrow::Borrow;
use std::clone::Clone;
use std::collections::HashMap;
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

use crate::config::Config;
use crate::Device;
use crate::device::{DeviceType, LcInfo, Status, UID};
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
    config: Arc<Config>,
    client: Client,
    device_mapper: DeviceMapper,
    devices: HashMap<UID, DeviceLock>,
    pub liqctld_update_client: Arc<LiqctldUpdateClient>,
}

impl LiquidctlRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        // todo: self generated certs
        Self::establish_connection(&client).await?;
        info!("Communication established with Liqctld.");
        let liqctld_update_client = LiqctldUpdateClient::new(client.clone()).await?;
        Ok(LiquidctlRepo {
            config,
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
            let driver_type = match self.map_driver_type(&device_response) {
                None => {
                    warn!("Device is currently not supported: {:?}", device_response.device_type);
                    continue;
                }
                Some(d_type) => d_type
            };
            self.liqctld_update_client.create_update_queue(&device_response.id).await;
            let mut device = Device::new(
                device_response.description,
                DeviceType::Liquidctl,
                device_response.id,
                Some(LcInfo {
                    driver_type,
                    firmware_version: None,
                    unknown_asetek: false,
                }),
                None,  // todo
                None,
                device_response.serial_number,
            );
            self.check_for_legacy_690(&mut device).await?;
            self.devices.insert(
                device.uid.clone(),
                Arc::new(RwLock::new(device)),
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

    fn map_driver_type(&self, device: &DeviceResponse) -> Option<BaseDriver> {
        BaseDriver::from_str(device.device_type.as_str())
            .ok()
            .filter(|driver| self.device_mapper.is_device_supported(driver))
    }

    fn map_status(&self,
                  driver_type: &BaseDriver,
                  lc_statuses: &LCStatus,
                  device_index: &u8,
    ) -> Status {
        let mut status_map: HashMap<String, String> = HashMap::new();
        for lc_status in lc_statuses {
            status_map.insert(lc_status.0.to_lowercase(), lc_status.1.clone());
        }
        self.device_mapper.extract_status(driver_type, &status_map, device_index)
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
                .replace("{}", device.type_index.to_string().as_str())
            )
            .json(&InitializeRequest { pump_mode: None })
            .send().await?
            .json::<StatusResponse>().await?;
        let device_index = device.type_index;
        let mut lc_info = device.lc_info.as_mut().expect("This should always be set for liquidctl devices");
        let init_status = self.map_status(
            &lc_info.driver_type,
            &status_response.status,
            &device_index,
        );
        lc_info.firmware_version = init_status.firmware_version.clone();
        device.set_status(init_status);
        Ok(())
    }

    async fn check_for_legacy_690(&self, device: &mut Device) -> Result<()> {
        let mut lc_info = device.lc_info.as_mut().expect("Should be present");
        if lc_info.driver_type == BaseDriver::Modern690Lc {
            if let Some(is_legacy690) = self.config.legacy690_ids().await?.get(&device.uid) {
                if *is_legacy690 {
                    let device_response = self.client.borrow()
                        .put(LIQCTLD_LEGACY690
                            .replace("{}", device.type_index.to_string().as_str())
                        )
                        .send().await?
                        .json::<DeviceResponse>().await?;
                    device.name = device_response.description.clone();
                    // let mut d_type = lc_info.driver_type;
                    lc_info.driver_type = self.map_driver_type(&device_response)
                        .expect("Should be Legacy690Lc");
                }
                // if is_legacy690 is false, then Modern690Lc is correct, nothing to do.
            } else {
                // if there is no setting for this device then we want to signal a request for
                // this info from the user.
                lc_info.unknown_asetek = true;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Repository for LiquidctlRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Liquidctl
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.call_initialize_concurrently().await;
        let mut init_devices = HashMap::new();
        for (uid, device) in self.devices.iter() {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Devices: {:#?}", init_devices);  // pretty output for easy reading
        } else {
            info!("Initialized Devices: {:?}", init_devices);
        }
        info!(
            "Time taken to initialize all liquidctl devices: {:?}", start_initialization.elapsed()
        );
        info!("Liquidctl Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    /// This works differently than by other repositories, because we preload the status in a
    /// liqctld_update_client queue so we don't lock the repositories for long periods of time.
    /// This keeps the response time for UI Device Status calls nice and low.
    async fn update_statuses(&self) -> Result<()> {
        for device_lock in self.devices.values() {
            let mut device = device_lock.write().await;
            let lc_status = self.liqctld_update_client
                .get_update_for_device(&device.type_index).await;
            if let Err(err) = lc_status {
                error!("{}", err);
                continue;
            }
            let status = self.map_status(
                &device.lc_info.as_ref()
                    .expect("Should always be present for LC devices")
                    .driver_type,
                &lc_status.unwrap(),
                &device.type_index,
            );
            device.set_status(status)
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        let quit_response = self.client
            .post(LIQCTLD_QUIT)
            .send().await?
            .json::<QuitResponse>().await?;
        info!("Liquidctl Repository Shutdown");
        return if quit_response.quit {
            info!("Quit Signal successfully sent to Liqctld");
            Ok(())
        } else {
            Err(anyhow!("Incorrect quit response from liqctld: {}", quit_response.quit))
        };
    }

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
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
    serial_number: Option<String>,
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

