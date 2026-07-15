/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::Not;
use std::time::{Duration, Instant};

use crate::rt::{sleep, timeout};
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::client::conn::http1::SendRequest;
use hyper::Response;
use hyper::{Request, Version};
use log::error;
use log::trace;
use log::warn;
use log::{debug, info};
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

/// The connection-driver task handle. On Tokio it is abortable; under compio it cancels when
/// dropped (compio's spawn cancels on handle drop). `SocketConnection::abort` unifies the two.
#[cfg(not(feature = "compio-rt"))]
type ConnDriver = tokio::task::JoinHandle<()>;
#[cfg(feature = "compio-rt")]
type ConnDriver = compio::runtime::JoinHandle<()>;

/// Connects a UDS to liqctld and wraps it in an IO type hyper can drive. Tokio uses `TokioIo`;
/// compio uses `cyper_core::HyperStream` over a compio `UnixStream`.
#[cfg(not(feature = "compio-rt"))]
async fn connect_liqctld_io(
) -> std::io::Result<impl hyper::rt::Read + hyper::rt::Write + Unpin + 'static> {
    let unix_stream = tokio::net::UnixStream::connect(LIQCTLD_SOCKET).await?;
    Ok(hyper_util::rt::TokioIo::new(unix_stream))
}
#[cfg(feature = "compio-rt")]
async fn connect_liqctld_io(
) -> std::io::Result<impl hyper::rt::Read + hyper::rt::Write + Unpin + 'static> {
    let unix_stream = compio::net::UnixStream::connect(LIQCTLD_SOCKET).await?;
    Ok(cyper_core::HyperStream::new(unix_stream))
}

/// Spawns the hyper connection-driver future on the active runtime and returns its handle. The
/// handle is held (not detached) so the connection can be aborted/cancelled later.
#[cfg(not(feature = "compio-rt"))]
fn spawn_conn_driver(fut: impl Future<Output = ()> + 'static) -> ConnDriver {
    tokio::task::spawn_local(fut)
}
#[cfg(feature = "compio-rt")]
fn spawn_conn_driver(fut: impl Future<Output = ()> + 'static) -> ConnDriver {
    compio::runtime::spawn(fut)
}

/// Ceiling on idle connections kept warm for reuse. NOT a cap on concurrent connections: the client
/// creates one whenever the pool is empty (`get_socket_connection`), so peak concurrency is bounded
/// only by the caller's own fan-out. This just bounds retention. Generous so many-device setups
/// reuse rather than re-handshake under sustained load; `reap_idle_connections` shrinks the pool
/// back to the working set once a burst subsides, so the headroom does not become a permanent cost.
const LIQCTLD_MAX_POOL_SIZE: usize = 32;
/// How long a pooled connection may sit unused before it is closed. Each open connection holds a
/// thread in liqctld's thread-per-connection server, so surplus connections left over from a burst
/// are reaped instead of held for the daemon's lifetime.
const LIQCTLD_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const LIQCTLD_EXPIRED_CONNECTION_RETRIES: usize = 7;
const LIQCTLD_RESPONSE_TIMEOUT_SECONDS: u64 = 15;
const LIQCTLD_SOCKET: &str = "/run/coolercontrold-liqctld.sock";
const LIQCTLD_HOST: &str = "127.0.0.1";
pub const LIQCTLD_CONNECTION_TRIES: usize = 10;
const LIQCTLD_HANDSHAKE: &str = "/handshake";
const LIQCTLD_DEVICES: &str = "/devices";
const LIQCTLD_LEGACY690: &str = "/devices/{}/legacy690";
const LIQCTLD_DIRECT_ACCESS: &str = "/devices/{}/direct-access";
const LIQCTLD_INITIALIZE: &str = "/devices/{}/initialize";
const LIQCTLD_STATUS: &str = "/devices/{}/status";
const LIQCTLD_FIXED_SPEED: &str = "/devices/{}/speed/fixed";
const LIQCTLD_SPEED_PROFILE: &str = "/devices/{}/speed/profile";
const LIQCTLD_CONTROL_MODE: &str = "/devices/{}/control-mode";
const LIQCTLD_COLOR: &str = "/devices/{}/color";
const LIQCTLD_SCREEN: &str = "/devices/{}/screen";
const LIQCTLD_SCAN: &str = "/devices/scan";
const LIQCTLD_QUIT: &str = "/quit";
const LIQCTLD_MAX_INIT_RETRIES: usize = 5;
const LIQCTLD_INIT_PAUSE_MS: u64 = 1_000;

/// A standard liquidctl status response (name, value, metric).
pub type LCStatus = Vec<(String, String, String)>;

/// `LiqctldClient` represents a client for interacting with a connection pool of socket connections.
pub struct LiqctldClient {
    connection_pool: RefCell<VecDeque<PooledConnection>>,
    /// Daemon-wide shutdown token. Used to skip request retries once shutting down, so a failing
    /// device does not add seconds of delay and log spam to the shutdown path.
    run_token: CancellationToken,
}

impl LiqctldClient {
    /// Establishes a socket connection to a coolercontrol-liqctld server, retries if there
    /// are connection errors, and saves the first connection in a queue of senders/connections
    /// for further communication.
    ///
    /// Returns:
    /// a Result containing either an instance of the struct or an error.
    pub async fn new(connection_tries: usize, run_token: CancellationToken) -> Result<Self> {
        let mut connection_pool = VecDeque::with_capacity(LIQCTLD_MAX_POOL_SIZE);
        let connection = Self::create_connection(connection_tries).await?;
        connection_pool.push_back(PooledConnection::new(connection));
        Ok(Self {
            connection_pool: RefCell::new(connection_pool),
            run_token,
        })
    }

    // private

    /// Attempts to establish a socket connection to a server and
    /// returns a `SocketConnection` if successful; otherwise it retries a specified number of times
    /// before returning an error.
    ///
    /// Returns:
    ///
    /// The function `create_connection` returns a `Result` containing a `SocketConnection` if the
    /// connection is successfully established. If the connection fails after the maximum number of
    /// retries, an error is returned.
    async fn create_connection(connection_tries: usize) -> Result<SocketConnection> {
        let mut retry_count = 0;
        // on startup liquidctl find_devices may take significant time which will keep the service
        // from communicating until that's complete. We need to retry to handle that, and since
        // the embedding of this service, it should be the only time we need to retry at startup.
        while retry_count < connection_tries {
            let io_stream = match connect_liqctld_io().await {
                Ok(io_stream) => io_stream,
                Err(err) => {
                    debug!(
                        "Could not establish socket connection to coolercontrol-liqctld, retry #{} - {err}",
                        retry_count + 1
                    );
                    Self::handle_retry(&mut retry_count, connection_tries).await;
                    continue;
                }
            };
            let (sender, connection) = match hyper::client::conn::http1::handshake(io_stream).await
            {
                Ok((sender, connection)) => (sender, connection),
                Err(err) => {
                    warn!(
                        "Could not handshake with coolercontrol-liqctld socket connection, retry #{} - {err}",
                        retry_count + 1
                    );
                    Self::handle_retry(&mut retry_count, connection_tries).await;
                    continue;
                }
            };
            // Keeps the connection open and drives http requests. We hold the handle so individual
            // connections can be aborted (Tokio) or cancelled on drop (compio).
            let connection_handle = spawn_conn_driver(async {
                if let Err(err) = connection.await {
                    error!("Unexpected Error: Connection to socket failed: {err:?}");
                }
            });
            trace!("Created a new socket connection.");
            return Ok(SocketConnection {
                sender,
                connection_handle,
            });
        }
        bail!(
            "Failed to connect to coolercontrol-liqctld after {retry_count} tries. \
            You can avoid this warning by disabling Liquidctl integration in the Daemon settings."
        );
    }

    /// Asynchronously increments the value of `retry_count` after waiting
    /// for 1 second.
    ///
    /// Arguments:
    ///
    /// * `retry_count`: A mutable reference to an unsigned integer variable representing the number of
    ///   retries.
    async fn handle_retry(retry_count: &mut usize, max_tries: usize) {
        *retry_count += 1;
        if *retry_count < max_tries {
            // If we still have retries left, then pause a moment.
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Attempts to retrieve a free socket connection from the connection pool,
    /// creating a new connection if necessary,
    /// and returning the `ConnectionUID` of the free connection.
    async fn get_socket_connection(&self) -> Result<SocketConnection> {
        self.reap_idle_connections();
        // LIFO: hand out the most-recently-returned connection so surplus connections from a burst
        // sink to the front of the pool and age out there. Round-robin (pop_front) reuse would keep
        // every connection warm and defeat idle reaping.
        if let Some(pooled) = self.connection_pool.borrow_mut().pop_back() {
            trace!("Found a free socket connection");
            return Ok(pooled.connection);
        }
        Self::create_connection(LIQCTLD_CONNECTION_TRIES).await
    }

    /// Closes connections idle past `LIQCTLD_IDLE_TIMEOUT`, releasing the thread each holds in
    /// liqctld's thread-per-connection server. The LIFO pool keeps idle connections at the front, so
    /// reap that stale prefix and stop at the first still-fresh one.
    fn reap_idle_connections(&self) {
        let now = Instant::now();
        let mut pool = self.connection_pool.borrow_mut();
        let stale = stale_prefix_len(
            pool.iter().map(|pooled| pooled.idle_since),
            now,
            LIQCTLD_IDLE_TIMEOUT,
        );
        for _ in 0..stale {
            if let Some(pooled) = pool.pop_front() {
                trace!("Reaping idle liqctld connection");
                pooled.connection.abort();
            }
        }
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
    async fn make_request<T>(&self, request: &Request<String>) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        for _ in 0..LIQCTLD_EXPIRED_CONNECTION_RETRIES {
            // If we run out of connections or timeout waiting for a connection, this will return Err:
            let mut socket_connection = self.get_socket_connection().await?;
            let response = match timeout(
                Duration::from_secs(LIQCTLD_RESPONSE_TIMEOUT_SECONDS),
                socket_connection.sender.send_request(request.clone()),
            )
            .await
            {
                Ok(call_response) => {
                    match call_response {
                        Ok(response) => response,
                        Err(err) => {
                            // this can happen occasionally (if a connection isn't used for a while)
                            debug!(
                                "Socket Connection no longer valid or closed. Aborting. {err:?}"
                            );
                            socket_connection.abort();
                            continue; // retry with a different connection
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        "Response timed out after {LIQCTLD_RESPONSE_TIMEOUT_SECONDS} seconds: {err:?}"
                    );
                    socket_connection.abort();
                    return Err(anyhow!(
                        "Response timed out, not retrying to avoid overloading service"
                    ));
                }
            };
            let lc_response = Self::collect_to_liqctld_response(response).await?;
            if self.connection_pool.borrow().len() < LIQCTLD_MAX_POOL_SIZE {
                self.connection_pool
                    .borrow_mut()
                    .push_back(PooledConnection::new(socket_connection));
            } else {
                debug!("liqctld connection pool full; closing the surplus connection.");
                socket_connection.abort();
            }
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

    /// Creates a new request builder for the `liqctld` service,
    /// with standard headers already set.
    fn request_builder() -> hyper::http::request::Builder {
        Request::builder()
            .header("Host", LIQCTLD_HOST)
            .header("Connection", "keep-alive")
            .version(Version::HTTP_11)
    }

    // public

    /// Sends a GET Handshake request to the liqctld service to verify requests are
    /// functioning within normal parameters.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn handshake(&self) -> Result<()> {
        let request = Self::request_builder()
            .uri(LIQCTLD_HANDSHAKE)
            .method("GET")
            .body(String::new())?;
        self.make_request::<IgnoredAny>(&request).await?;
        Ok(())
    }

    /// Gets a list of all devices connected to the system.
    ///
    /// Returns:
    ///
    /// a Result object with a `DevicesResponse` as the Ok variant.
    pub async fn get_all_devices(&self) -> Result<DevicesResponse> {
        let request = Self::request_builder()
            .uri(LIQCTLD_DEVICES)
            .method("GET")
            .body(String::new())?;
        self.make_request(&request).await
    }

    /// Performs a fresh scan for currently connected liquidctl devices without
    /// modifying the cached device state in the Python service.
    pub async fn scan_devices(&self) -> Result<DevicesResponse> {
        let request = Self::request_builder()
            .uri(LIQCTLD_SCAN)
            .method("GET")
            .body(String::new())?;
        self.make_request(&request).await
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
        let request = Self::request_builder()
            .uri(LIQCTLD_STATUS.replace("{}", &device_index.to_string()))
            .method("GET")
            .body(String::new())?;
        self.make_request(&request).await
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
    /// A `Result` with a `StatusResponse` as the success type.
    pub async fn initialize_device(
        &self,
        device_index: &u8,
        pump_mode: Option<String>,
    ) -> Result<StatusResponse> {
        let request_body = serde_json::to_string(&InitializeRequest { pump_mode })?;
        let request = Self::request_builder()
            .uri(LIQCTLD_INITIALIZE.replace("{}", &device_index.to_string()))
            .method("POST")
            .body(request_body)?;
        let mut response = self.make_request(&request).await;
        if response.is_err() {
            for _ in 1..LIQCTLD_MAX_INIT_RETRIES {
                // Do not retry while shutting down: the device is already failing, and each retry
                // only adds a 1s delay plus log spam to the shutdown path (e.g. the LCD shutdown
                // image). One failed attempt is enough.
                if self.run_token.is_cancelled() {
                    return response;
                }
                sleep(Duration::from_millis(LIQCTLD_INIT_PAUSE_MS)).await;
                info!("Retrying liquidctl device #{device_index} initialization request.");
                response = self.make_request(&request).await;
                if response.is_ok() {
                    return response;
                }
            }
            warn!(
                "Failed to successfully initialize liquidctl device after {LIQCTLD_MAX_INIT_RETRIES} tries."
            );
        }
        response
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
        let request = Self::request_builder()
            .uri(LIQCTLD_LEGACY690.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(String::new())?;
        self.make_request(&request).await
    }

    /// Forces direct access for a particular device.
    /// This should be called before initializing the device.
    ///
    /// Arguments:
    ///
    /// * `device_index`: The `device_index` parameter is a reference to an unsigned 8-bit integer
    ///   (`&u8`). It represents the index of a device.
    ///
    /// Returns:
    ///
    /// a Result object with a value of `DeviceResponse`.
    pub async fn put_direct_access(&self, device_index: &u8) -> Result<()> {
        let request = Self::request_builder()
            .uri(LIQCTLD_DIRECT_ACCESS.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(String::new())?;
        self.make_request::<IgnoredAny>(&request).await?;
        Ok(())
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
        let request = Self::request_builder()
            .uri(LIQCTLD_FIXED_SPEED.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(&request).await?;
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
        let request = Self::request_builder()
            .uri(LIQCTLD_SPEED_PROFILE.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(&request).await?;
        Ok(())
    }

    pub async fn put_control_mode(
        &self,
        device_index: &u8,
        channel_name: &str,
        control_mode: &str,
    ) -> Result<()> {
        let request_body = serde_json::to_string(&ControlModeRequest {
            channel: channel_name.to_string(),
            desired_state: control_mode.to_string(),
        })?;
        let request = Self::request_builder()
            .uri(LIQCTLD_CONTROL_MODE.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(&request).await?;
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
        let request = Self::request_builder()
            .uri(LIQCTLD_COLOR.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(&request).await?;
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
        let request = Self::request_builder()
            .uri(LIQCTLD_SCREEN.replace("{}", &device_index.to_string()))
            .method("PUT")
            .body(request_body)?;
        self.make_request::<IgnoredAny>(&request).await?;
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
        self.make_request::<IgnoredAny>(&request).await?;
        Ok(())
    }

    /// Shuts down all connections in a connection pool and clears the pool.
    pub fn shutdown(&self) {
        for pooled in self.connection_pool.borrow_mut().drain(..) {
            pooled.connection.abort();
        }
    }
}

/// A pooled connection and the instant it last became idle (was returned to the pool). The pool is
/// a LIFO stack: `get_socket_connection` takes from the back, so surplus connections sink to the
/// front and age there until `reap_idle_connections` closes them.
struct PooledConnection {
    connection: SocketConnection,
    idle_since: Instant,
}

impl PooledConnection {
    fn new(connection: SocketConnection) -> Self {
        Self {
            connection,
            idle_since: Instant::now(),
        }
    }
}

/// Count of leading entries idle longer than `timeout`, assuming oldest-idle-first ordering. Stops
/// at the first fresh entry, which the LIFO pool guarantees means the rest are fresh too.
fn stale_prefix_len(
    idle_since: impl Iterator<Item = Instant>,
    now: Instant,
    timeout: Duration,
) -> usize {
    let mut count = 0;
    for idle_at in idle_since {
        if now.duration_since(idle_at) > timeout {
            count += 1;
        } else {
            break;
        }
    }
    count
}

struct SocketConnection {
    sender: SendRequest<String>,
    connection_handle: ConnDriver,
}

impl SocketConnection {
    /// Tears down the connection's driver task. On Tokio it aborts the join handle; under compio
    /// dropping `self` (and its handle) cancels the task, so this just consumes `self`.
    #[allow(clippy::needless_pass_by_value)] // consumes self so the compio handle drops (cancels)
    fn abort(self) {
        #[cfg(not(feature = "compio-rt"))]
        self.connection_handle.abort();
        // Dropping a compio JoinHandle cancels its task; do so explicitly.
        #[cfg(feature = "compio-rt")]
        drop(self.connection_handle);
    }
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
struct ControlModeRequest {
    channel: String,
    desired_state: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_prefix_len_counts_only_the_leading_expired_run() {
        // Goal: reaping closes the run of oldest-idle connections that have exceeded the timeout and
        // stops at the first still-fresh one. Since the pool is LIFO (fresh connections kept at the
        // back), a fresh entry means every later entry is fresh too, so the scan can stop early.
        // Method: build instants relative to a fixed `now` and assert the counted prefix.
        let now = Instant::now();
        let old = now - Duration::from_secs(120);
        let fresh = now - Duration::from_secs(1);
        let timeout = Duration::from_secs(30);
        assert_eq!(
            stale_prefix_len([old, old, fresh, fresh].into_iter(), now, timeout),
            2
        );
        assert_eq!(
            stale_prefix_len([fresh, fresh].into_iter(), now, timeout),
            0
        );
        assert_eq!(
            stale_prefix_len([old, old, old].into_iter(), now, timeout),
            3
        );
        assert_eq!(stale_prefix_len(std::iter::empty(), now, timeout), 0);
        // A fresh entry halts the scan even if an older one follows; the straggler is reaped on a
        // later pass once the fresh ones ahead of it are consumed.
        assert_eq!(
            stale_prefix_len([old, fresh, old].into_iter(), now, timeout),
            1
        );
    }
}
