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

use std::collections::{HashMap, VecDeque};

use anyhow::{anyhow, Context, Result};
use const_format::concatcp;
use log::{debug, error};
use reqwest::Client;
use tokio::sync::RwLock;
use tokio::time::Instant;
use zbus::export::futures_util::future::join_all;

use crate::repositories::liquidctl::liquidctl_repo::{LIQCTLD_ADDRESS, StatusResponse};

const LIQCTLD_STATUS: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/status");

type LCStatus = Vec<(String, String, String)>;

/// We use an external client here so that we can preload status updates without write-blocking
/// access to all the Repos.
pub struct LiqctldUpdateClient {
    client: Client,
    queues: RwLock<HashMap<u8, VecDeque<LCStatus>>>,
}

impl LiqctldUpdateClient {
    pub async fn new(client: Client) -> Result<Self> {
        Ok(Self {
            client,
            queues: RwLock::new(HashMap::new()),
        })
    }

    pub async fn create_update_queue(&self, device_id: &u8) {
        self.queues.write().await.insert(device_id.clone(), VecDeque::with_capacity(1));
    }

    pub async fn get_update_for_device(&self, device_id: &u8) -> Result<LCStatus> {
        match self.queues.write().await.get_mut(device_id) {
            Some(queue) => {
                match queue.pop_front() {
                    Some(status) => Ok(status),
                    None => Err(anyhow!("Queue is empty for device_id: {}", device_id))
                }
            }
            None => Err(anyhow!("No queue exists for this device_id: {}:", device_id))
        }
    }

    pub async fn preload_statuses(&self) {
        debug!("Updating all Liquidctl device statuses");
        let start_update = Instant::now();
        let mut queues = self.queues.write().await;
        let mut futures = vec![];
        for (device_id, queue) in queues.iter_mut() {
            futures.push(
                self.add_status_to_queue(self.call_status(device_id).await, queue)
            )
        }
        join_all(futures).await;
        debug!(
            "Time taken to update status for all liquidctl devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn add_status_to_queue(&self, status: Result<LCStatus>, queue: &mut VecDeque<LCStatus>) {
        match status {
            Ok(status) => queue.push_back(status),
            Err(err) => error!("Error getting status from device: {}", err)
        }
    }
    async fn call_status(&self, device_id: &u8) -> Result<LCStatus> {
        let status_response = self.client
            .get(LIQCTLD_STATUS.replace("{}", device_id.to_string().as_str()))
            .send().await
            .with_context(|| format!("Trying to get status for device_id: {}", device_id))?
            .json::<StatusResponse>().await?;
        // debug!("Status updated for LC Device #{} with: {:?}", &device_id, &status_response);
        Ok(status_response.status)
    }
}
