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

use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use zmq::{Message, Socket};

const TMP_SOCKET_DIR: &str = "/tmp/coolercontrol.sock";
const TIMEOUT: i32 = 5_000;  // millis

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    command: String,
    parameters: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    success: String,
    error: String,
}

pub struct Client {
    context: zmq::Context,
    socket: Socket,
}

impl Client {
    pub fn new() -> Result<Client> {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::REQ).unwrap();
        // todo: check is running as systemd daemon and use FD... appears different in the rust impl
        socket.set_connect_timeout(TIMEOUT)?;
        socket.set_rcvtimeo(TIMEOUT)?;
        socket.set_linger(TIMEOUT)?;  // do not wait forever when shutting the connection down
        socket.connect(format!("ipc://{}", TMP_SOCKET_DIR).as_str())
            .with_context(|| format!("Could not open socket: {}", TMP_SOCKET_DIR))?;
        info!("connected to ipc socket");
        Ok(Client {
            context,
            socket,
        })
    }

    pub fn handshake(&self) -> Result<()> {
        let request = Request {
            command: "handshake".to_string(),
            parameters: "".to_string(),
        };
        let handshake_json: String = serde_json::to_string(&request)
            .with_context(|| format!("Object serialization failed: {:?}", request))?;

        self.socket.send(handshake_json.as_str(), 0)
            .with_context(|| format!("Sending of message failed: {:?}", request))?;

        debug!("Handshake sent: {:?}", handshake_json);

        let mut response_msg = Message::new();
        self.socket.recv(&mut response_msg, 0)
            .context("Timed-out waiting for response from handshake")?;

        let response_msg_str = response_msg.as_str()
            .context("Error trying to stringify response")?;
        let response: Response = serde_json::from_str(response_msg_str)
            .with_context(|| format!("Could not deserialize response: {:?}", response_msg_str))?;
        debug!("Handshake response received: {:?}", response);

        if response.success == request.command {
            info!("Handshake successful");
            Ok(())
        } else { Err(anyhow!("Unexpected handshake response: {:?}", response)) }
    }

    pub fn quit(&self) -> Result<()> {
        let request = Request {
            command: "quit".to_string(),
            parameters: "".to_string(),
        };
        let quit_json: String = serde_json::to_string(&request)
            .with_context(|| format!("Object serialization failed: {:?}", request))?;

        self.socket.send(quit_json.as_str(), 0)
            .with_context(|| format!("Sending of message failed: {:?}", request))?;
        debug!("Quit signal sent");

        let mut response_msg = Message::new();
        self.socket.recv(&mut response_msg, 0)
            .context("Error waiting for response from handshake")?;

        let response_msg_str = response_msg.as_str()
            .context("Error trying to stringify response")?;
        let response: Response = serde_json::from_str(response_msg_str)
            .with_context(|| format!("Could not deserialize response: {:?}", response_msg_str))?;
        debug!("Quit response received: {:?}", response);

        if response.success == request.command {
            info!("Successfully sent quit command to Liqctld");
            Ok(())
        } else { Err(anyhow!("Unexpected quit response: {:?}", response)) }
    }

    pub fn find_devices(&self) -> Result<String> {
        let request = Request {
            command: "find_devices".to_string(),
            parameters: "".to_string(),
        };
        let quit_json: String = serde_json::to_string(&request)
            .with_context(|| format!("Object serialization failed: {:?}", request))?;

        self.socket.send(quit_json.as_str(), 0)
            .with_context(|| format!("Sending of message failed: {:?}", request))?;
        debug!("find devices signal sent");

        let mut response_msg = Message::new();
        self.socket.recv(&mut response_msg, 0)
            .context("Error waiting for response from liqctld find_devices")?;

        let response_msg_str = response_msg.as_str()
            .context("Error trying to stringify response")?;
        let response: Response = serde_json::from_str(response_msg_str)
            .with_context(|| format!("Could not deserialize response: {:?}", response_msg_str))?;
        debug!("Find Devices response received: {:?}", response);

        if !response.error.is_empty() {
            Err(anyhow!("Error trying to initialize devices: {}", response.error))
        } else {
            info!("Liquidctl device initialization complete");
            Ok(response.success)
        }
    }

    pub fn get_status(&self, device_id: &u8) -> Result<String> {
        let request = Request {
            command: "get_status".to_string(),
            parameters: device_id.to_string(),
        };
        let quit_json: String = serde_json::to_string(&request)
            .with_context(|| format!("Object serialization failed: {:?}", request))?;

        self.socket.send(quit_json.as_str(), 0)
            .with_context(|| format!("Sending of message failed: {:?}", request))?;
        debug!("get status signal sent");

        let mut response_msg = Message::new();
        self.socket.recv(&mut response_msg, 0)
            .context("Error waiting for response from liqctld get_status")?;

        let response_msg_str = response_msg.as_str()
            .context("Error trying to stringify response")?;
        let response: Response = serde_json::from_str(response_msg_str)
            .with_context(|| format!("Could not deserialize response: {:?}", response_msg_str))?;
        debug!("Get Status response received: {:?}", response);

        if !response.error.is_empty() {
            Err(anyhow!("Error trying to initialize devices: {}", response.error))
        } else {
            info!("Liquidctl device initialization complete");
            Ok(response.success)
        }
    }
}
