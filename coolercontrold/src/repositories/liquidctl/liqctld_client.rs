/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::ops::Not;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use hashlink::LinkedHashMap;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::client::conn::http1::SendRequest;
use hyper::Request;
use hyper::Response;
use hyper_util::rt::TokioIo;
use log::debug;
use log::error;
use log::trace;
use log::warn;
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde::Serialize;
use tokio::net::UnixStream;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use uuid::Uuid;

const LIQCTLD_MAX_POOL_SIZE: usize = 10;
const LIQCTLD_MAX_POOL_RETRIES: usize = 7;
const LIQCTLD_EXPIRED_CONNECTION_RETRIES: usize = 7;
const LIQCTLD_SOCKET: &str = "/run/coolercontrol-liqctld.sock";
const LIQCTLD_HOST: &str = "127.0.0.1";
pub const LIQCTLD_CONNECTION_TRIES: usize = 3;
const LIQCTLD_HANDSHAKE: &str = "/handshake";
const LIQCTLD_DEVICES: &str = "/devices";
const LIQCTLD_LEGACY690: &str = "/devices/{}/legacy690";
const LIQCTLD_INITIALIZE: &str = "/devices/{}/initialize";
const LIQCTLD_STATUS: &str = "/devices/{}/status";
const LIQCTLD_FIXED_SPEED: &str = "/devices/{}/speed/fixed";
const LIQCTLD_SPEED_PROFILE: &str = "/devices/{}/speed/profile";
const LIQCTLD_COLOR: &str = "/devices/{}/color";
const LIQCTLD_SCREEN: &str = "/devices/{}/screen";
const LIQCTLD_QUIT: &str = "/quit";

/// A standard liquidctl status response (name, value, metric).
pub type LCStatus = Vec<(String, String, String)>;
// tokio::Mutex is used to make sure we're only making one request at a time per connection
type SocketConnectionLock = Mutex<SocketConnection>;
type ConnectionUID = String;

/// `LiqctldClient` represents a client for interacting with a connection pool of socket connections.
///
/// Properties:
///
/// * `connection_pool`: The `connection_pool` property is a vector of `SocketConnectionLock` objects,
///   wrapped in a `RefCell`.
pub struct LiqctldClient {
    // tokio::RwLock is proper here as we're holding a lock over await points for multiple concurrent connections
    connection_pool: RwLock<LinkedHashMap<ConnectionUID, SocketConnectionLock>>,
}

impl LiqctldClient {
    /// Establishes a socket connection to a coolercontrol-liqctld server, retries if there
    /// are connection errors, and saves the first connection in a pool of senders/connections
    /// for further communication.
    ///
    /// Returns:
    /// a Result containing either an instance of the struct or an error.
    pub async fn new(connection_tries: usize) -> Result<Self> {
        let mut connection_pool = LinkedHashMap::with_capacity(LIQCTLD_MAX_POOL_SIZE);
        let (uid, connection) = Self::create_connection(connection_tries).await?;
        connection_pool.insert(uid, Mutex::new(connection));
        Ok(Self {
            connection_pool: RwLock::new(connection_pool),
        })
    }

    // private

    /// Attempts to establish a socket connection to a server and
    /// returns a `SocketConnection` if successful, otherwise it retries a specified number of times
    /// before returning an error.
    ///
    /// Returns:
    ///
    /// The function `create_connection` returns a `Result` containing a `SocketConnection` if the
    /// connection is successfully established. If the connection fails after the maximum number of
    /// retries, an error is returned.
    async fn create_connection(
        connection_tries: usize,
    ) -> Result<(ConnectionUID, SocketConnection)> {
        let mut retry_count = 0;
        while retry_count < connection_tries {
            let unix_stream = match UnixStream::connect(LIQCTLD_SOCKET).await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!("Could not establish socket connection to coolercontrol-liqctld, retry #{} - {err}",retry_count + 1);
                    Self::handle_retry(&mut retry_count).await;
                    continue;
                }
            };
            let io_stream = TokioIo::new(unix_stream);
            // When hyper_util has a more mature higher-level Client impl, we can use that instead.
            let (sender, connection) = match hyper::client::conn::http1::handshake(io_stream).await
            {
                Ok((sender, connection)) => (sender, connection),
                Err(err) => {
                    error!("Could not handshake with coolercontrol-liqctld socket connection, retry #{} - {err}", retry_count + 1);
                    Self::handle_retry(&mut retry_count).await;
                    continue;
                }
            };
            // keeps the connection open and drives http requests
            // Tokio::task::spawn here is preferred as we can abort() individual futures
            // and since it's only for the hyper Connection, is fine to use here.
            let connection_handle = tokio::task::spawn_local(async {
                if let Err(err) = connection.await {
                    error!("Unexpected Error: Connection to socket failed: {err:?}");
                }
            });
            return Ok((
                Uuid::new_v4().to_string(),
                SocketConnection {
                    sender,
                    connection_handle,
                },
            ));
        }
        bail!("Failed to connect to coolercontrol-liqctld after {retry_count} tries");
    }

    /// Asynchronously increments the value of `retry_count` after waiting
    /// for 1 second.
    ///
    /// Arguments:
    ///
    /// * `retry_count`: A mutable reference to an unsigned integer variable representing the number of
    ///   retries.
    async fn handle_retry(retry_count: &mut usize) {
        sleep(Duration::from_secs(1)).await;
        *retry_count += 1;
    }

    /// Attempts to retrieve a free socket connection from the connection pool,
    /// creating a new connection if necessary,
    /// and returning the `ConnectionUID` of the free connection.
    async fn get_socket_connection(&self) -> Result<ConnectionUID> {
        for _ in 0..LIQCTLD_MAX_POOL_RETRIES {
            for (c_id, s_lock) in self.connection_pool.read().await.iter() {
                if s_lock.try_lock().is_err() {
                    trace!("The {c_id} socket connection is busy, trying another.");
                    continue;
                }
                trace!("Found a free socket connection: {c_id}");
                return Ok(c_id.to_owned());
            }
            {
                let mut pool = self.connection_pool.write().await;
                if pool.len() < LIQCTLD_MAX_POOL_SIZE {
                    let (c_id, connection) =
                        Self::create_connection(LIQCTLD_CONNECTION_TRIES).await?;
                    pool.insert(c_id.clone(), Mutex::new(connection));
                    trace!(
                        "Created a new socket connection and added it to the pool now of {}.",
                        pool.len()
                    );
                    return Ok(c_id);
                }
            }
            warn!(
                "Socket connection pool full & busy, waiting for a connection to become available."
            );
            sleep(Duration::from_millis(100)).await;
        }
        bail!("Failed to get a free liqctld connection after {LIQCTLD_MAX_POOL_RETRIES} tries");
    }

    /// Sends a request to a socket connection, handles errors, and returns the deserialized response.
    ///
    /// Arguments:
    ///
    /// * `request`: The `request` parameter is of type `Request<String>`. It represents a request to be
    ///   sent to a socket connection. The `String` type parameter indicates the body of the request,
    ///   which is expected to be in JSON format.
    ///
    /// Returns:
    ///
    /// a Result<T>, where T is the deserialized response from the request.
    async fn make_request<T>(&self, request: Request<String>) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        for _ in 0..LIQCTLD_EXPIRED_CONNECTION_RETRIES {
            // If we run out of connections or timeout, this will return Err:
            let c_id = self.get_socket_connection().await?;
            let pool_read_lock = self.connection_pool.read().await;
            let c_lock = pool_read_lock.get(&c_id).expect("Connection should exist");
            let Ok(response) = c_lock
                .lock()
                .await
                .sender
                .send_request(request.clone())
                .await
            else {
                // this can happen semi-regularly (if a connection isn't used for a while)
                debug!("Socket Connection no longer valid. Closing.");
                c_lock.lock().await.connection_handle.abort();
                drop(pool_read_lock);
                // tokio write lock is fair, but likely will wait until other requests are finished:
                self.connection_pool.write().await.remove(&c_id);
                continue; // retry with a different connection
            };
            let lc_response = Self::collect_to_liqctld_response(response).await?;
            return Ok(serde_json::from_str(&lc_response.body)?);
        }
        Err(anyhow!(
            "Failed to get a free or new liqctld connection after {LIQCTLD_EXPIRED_CONNECTION_RETRIES} tries"
        ))
    }

    /// Converts a `Response<Incoming>` object into a `LiqctldResponse` object,
    /// handling any errors that may occur.
    ///
    /// Arguments:
    ///
    /// * `response`: The `response` parameter is of type `Response<Incoming>`. It represents the HTTP
    ///   response received from a server.
    ///
    /// Returns:
    ///
    /// a Result<LiqctldResponse>.
    async fn collect_to_liqctld_response(response: Response<Incoming>) -> Result<LiqctldResponse> {
        let (head, body_incoming) = response.into_parts();
        let body = String::from_utf8(body_incoming.collect().await?.to_bytes().into())?;
        trace!("Response Head: {head:?}");
        trace!("Response Body: {body:#?}");
        if head.status.is_success().not() {
            return Err(anyhow!(
                "Liqctld Request failed with status:{} - Body: {body}",
                head.status,
            ));
        }
        Ok(LiqctldResponse { body })
    }

    // public

    /// Sends a GET Handshake request to the liqctld service to verify requests are
    /// functioning within normal parameters.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn handshake(&self) -> Result<()> {
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_HANDSHAKE)
            .method("GET")
            .body(String::new())?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// Gets a list of all devices connected to the system.
    ///
    /// Returns:
    ///
    /// a Result object with a `DevicesResponse` as the Ok variant.
    pub async fn get_all_devices(&self) -> Result<DevicesResponse> {
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_DEVICES)
            .method("GET")
            .body(String::new())?;
        self.make_request(request).await
    }

    /// Gets the status of a specific device.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is a reference to an unsigned 8-bit integer
    ///   (`u8`). It is used to specify the index of the device for which the status is being requested.
    ///
    /// Returns:
    ///
    /// a `Result` with a `StatusResponse` object.
    pub async fn get_status(&self, device_index: &u8) -> Result<StatusResponse> {
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_STATUS.replace("{}", &device_index.to_string()))
            .method("GET")
            .body(String::new())?;
        self.make_request(request).await
    }

    /// Initializes a specific device.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is a reference to an unsigned 8-bit integer
    ///   (`u8`). It is used to identify the index of the device that needs to be initialized.
    /// * `pump_mode`: The `pump_mode` parameter is an optional string that represents the desired mode
    ///   of the pump. It is used as a parameter in the `InitializeRequest` struct, which is then
    ///   serialized to JSON and included in the request body.
    ///
    /// Returns:
    ///
    /// a `Result` with a `StatusResponse` as the success type.
    pub async fn initialize_device(
        &self,
        device_index: &u8,
        pump_mode: Option<String>,
    ) -> Result<StatusResponse> {
        let request_body = serde_json::to_string(&InitializeRequest { pump_mode })?;
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_INITIALIZE.replace("{}", &device_index.to_string()))
            .method("POST")
            .body(request_body)?;
        self.make_request(request).await
    }

    /// Sets a particular device to legacy 690 mode. (Old Krakens/EVGA CLC)
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is a reference to an unsigned 8-bit integer
    ///   (`&u8`). It represents the index of a device.
    ///
    /// Returns:
    ///
    /// a Result object with a value of `DeviceResponse`.
    pub async fn put_legacy690(&self, device_index: &u8) -> Result<DeviceResponse> {
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_LEGACY690.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(String::new())?;
        self.make_request(request).await
    }

    /// Sets a particular device channel to a fixed speed.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is the index or identifier of the device you want
    ///   to control. It is of type `u8`, which means it is an unsigned 8-bit integer.
    /// * `channel_name`: The `channel_name` parameter is a string that represents the name of the
    ///   channel for which you want to set the fixed speed.
    /// * `fixed_speed`: The `fixed_speed` parameter represents the desired fixed speed value for a
    ///   specific channel on a device. It is of type `u8`, which means it can hold values from 0 to 255.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn put_fixed_speed(
        &self,
        device_index: &u8,
        channel_name: &str,
        fixed_speed: u8,
    ) -> Result<()> {
        let request_body = serde_json::to_string(&FixedSpeedRequest {
            channel: channel_name.to_string(),
            duty: fixed_speed,
        })?;
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_FIXED_SPEED.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// Sets a particular device channel to a speed profile.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is the index of the device for which you want to
    ///   set the speed profile. It is of type `u8`, which means it is an unsigned 8-bit integer.
    /// * `channel_name`: The `channel_name` parameter is a string that represents the name of the
    ///   channel for which the speed profile is being set.
    /// * `profile`: The `profile` parameter is a vector of tuples, where each tuple contains two
    ///   values: a `f64` representing a temperature point, and a `u8` representing a speed level. This
    ///   profile represents the desired speed levels for different temperature points.
    /// * `temperature_sensor`: The `temperature_sensor` parameter is an optional parameter that
    ///   represents the temperature sensor to be used for the speed profile. It is of type `Option<u8>`,
    ///   which means it can either be `Some(u8)` where `u8` is the index of the temperature sensor, or
    ///   `None`.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn put_speed_profile(
        &self,
        device_index: &u8,
        channel_name: &str,
        profile: &[(f64, u8)],
        temperature_sensor: Option<u8>,
    ) -> Result<()> {
        let request_body = serde_json::to_string(&SpeedProfileRequest {
            channel: channel_name.to_string(),
            profile: profile.to_vec(),
            temperature_sensor,
        })?;
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_SPEED_PROFILE.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// Sets a particular device channel to the given color setting.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is the index or identifier of the device you want
    ///   to control. It is of type `u8`, which means it is an unsigned 8-bit integer.
    /// * `channel_name`: The `channel_name` parameter is a string that represents the name of the
    ///   channel you want to set the color for.
    /// * `mode`: The `mode` parameter in the `put_color` function represents the mode in which the
    ///   colors will be displayed on the device. It is a string that specifies the desired mode, such as
    ///   "solid", "fade", "blink", etc. The specific modes available may depend on the device or library
    /// * `colors`: The `colors` parameter is a vector of tuples representing RGB color values. Each
    ///   tuple consists of three `u8` values representing the red, green, and blue components of the
    ///   color. For example, `(255, 0, 0)` represents the color red.
    /// * `time_per_color`: The `time_per_color` parameter is an optional parameter that specifies the
    ///   duration (in seconds) for which each color in the `colors` vector should be displayed. If this
    ///   parameter is not provided, the default duration will be used.
    /// * `speed`: The `speed` parameter is an optional parameter that specifies the speed at which the
    ///   colors should transition. It is of type `Option<String>`, which means it can either be
    ///   `Some(speed_value)` or `None`. If it is `Some(speed_value)`, the `speed_value` should be
    /// * `direction`: The `direction` parameter is an optional string that specifies the direction of
    ///   the color change. It can have values like "forward", "backward", "clockwise",
    ///   "counterclockwise", etc., depending on the specific implementation or requirements of the system
    ///   you are working with.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn put_color(
        &self,
        device_index: &u8,
        channel_name: &str,
        mode: &str,
        colors: Vec<(u8, u8, u8)>,
        time_per_color: Option<u8>,
        speed: Option<String>,
        direction: Option<String>,
    ) -> Result<()> {
        let request_body = serde_json::to_string(&ColorRequest {
            channel: channel_name.to_string(),
            mode: mode.to_string(),
            colors,
            time_per_color,
            speed,
            direction,
        })?;
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_COLOR.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// Sets a particular device channel to the given screen settings.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is the index of the device you want to put the
    ///   screen for. It is of type `u8`, which means it is an unsigned 8-bit integer.
    /// * `channel_name`: The `channel_name` parameter is a string that represents the name of the
    ///   channel for the screen. It is used to identify the specific channel on the device where the
    ///   screen is located.
    /// * `mode`: The `mode` parameter in the `put_screen` function is a string that represents the
    ///   desired mode for the screen. Current values are "gif", "static", "orientation", and "brightness".
    /// * `value`: The `value` parameter is an optional `String` that represents the value to be set for
    ///   the screen mode.
    ///
    /// Returns:
    ///
    /// a Result<()> type, which means it either returns Ok(()) if the operation is successful or an
    /// error if there is any issue.
    pub async fn put_screen(
        &self,
        device_index: &u8,
        channel_name: &str,
        mode: &str,
        value: Option<String>,
    ) -> Result<()> {
        let request_body = serde_json::to_string(&ScreenRequest {
            channel: channel_name.to_string(),
            mode: mode.to_string(),
            value,
        })?;
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_SCREEN.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// This shuts the coolercontrol-liqctld service down.
    ///
    /// Returns:
    ///
    /// a Result<()> type, which means it either returns Ok(()) if the operation is successful or an
    /// error if there is any issue.
    pub async fn post_quit(&self) -> Result<()> {
        let request = Request::builder()
            .header("Host", LIQCTLD_HOST)
            .uri(LIQCTLD_QUIT)
            .method("POST")
            .body(String::new())?;
        self.make_request::<IgnoredAny>(request).await?;
        Ok(())
    }

    /// Asynchronously shuts down all connections in a connection pool and clears the pool.
    pub async fn shutdown(&self) {
        let mut pool = self.connection_pool.write().await;
        for connection in pool.values() {
            connection.lock().await.connection_handle.abort();
        }
        pool.clear();
    }

    /// Checks if the connection pool is empty and returns a boolean value
    /// indicating whether there are active connections.
    ///
    /// Returns:
    ///
    /// a boolean value.
    pub async fn is_connected(&self) -> bool {
        self.connection_pool.read().await.is_empty().not()
    }
}

struct SocketConnection {
    sender: SendRequest<String>,
    connection_handle: JoinHandle<()>,
}

#[derive(Debug, Clone)]
struct LiqctldResponse {
    // status: StatusCode, // if needed later
    body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceResponse {
    pub id: u8,
    pub description: String,
    /// Also called `DriverName`
    pub device_type: String,
    pub serial_number: Option<String>,
    pub properties: DeviceProperties,
    pub liquidctl_version: Option<String>,
    pub hid_address: Option<String>,
    pub hwmon_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProperties {
    pub speed_channels: Vec<String>,
    pub color_channels: Vec<String>,
    pub supports_cooling: Option<bool>,
    pub supports_cooling_profiles: Option<bool>,
    pub supports_lighting: Option<bool>,
    pub led_count: Option<u8>,
    pub lcd_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: LCStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    pump_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixedSpeedRequest {
    channel: String,
    duty: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpeedProfileRequest {
    channel: String,
    // INFO: Some liquidctl device drivers need ints. coolercontrol-liqctld will handle this.
    profile: Vec<(f64, u8)>,
    temperature_sensor: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ColorRequest {
    channel: String,
    mode: String,
    colors: Vec<(u8, u8, u8)>,
    time_per_color: Option<u8>,
    speed: Option<String>,
    direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScreenRequest {
    channel: String,
    mode: String,
    value: Option<String>,
}
