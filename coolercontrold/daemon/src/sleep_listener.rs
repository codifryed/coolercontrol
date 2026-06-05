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

use crate::sidecar::Sidecar;
use crate::ENV_DBUS;
use log::{info, warn};
use std::env;
use std::ops::Not;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{Connection, Proxy};

const SLEEP_DEFAULT_BUS_NAME: &str = "org.freedesktop.login1";
const SLEEP_OBJECTPATH: &str = "/org/freedesktop/login1";
const SLEEP_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SIGNAL_PREPARE_FOR_SLEEP: &str = "PrepareForSleep";
// The whole DBus handshake (connect + proxy + AddMatch for PrepareForSleep) should complete in
// tens of milliseconds on a healthy system. We cap it so a wedged logind/dbus-broker can't block
// the listener from ever connecting; on timeout we simply run deaf (the flags stay false).
const DBUS_SETUP_TIMEOUT_S: u64 = 5;

/// Tracks system sleep/resume state for the main loop. The dbus connection and signal loop run on
/// the Tokio sidecar (zbus needs a Tokio reactor); the flags are shared back to the main thread via
/// `Arc<AtomicBool>` and polled each tick. If dbus is disabled or fails to connect, the flags stay
/// false and the listener is effectively deaf.
pub struct SleepListener {
    preparing_to_sleep: Arc<AtomicBool>,
    resuming: Arc<AtomicBool>,
}

impl SleepListener {
    pub fn new(run_token: CancellationToken, sidecar: &Sidecar) -> Self {
        let preparing_to_sleep = Arc::new(AtomicBool::new(false));
        let resuming = Arc::new(AtomicBool::new(false));
        if dbus_listener_enabled().not() {
            info!("DBUS sleep listener disabled.");
            return Self {
                preparing_to_sleep,
                resuming,
            };
        }
        let preparing = Arc::clone(&preparing_to_sleep);
        let waking = Arc::clone(&resuming);
        sidecar.spawn(move || run_listener(run_token, preparing, waking));
        Self {
            preparing_to_sleep,
            resuming,
        }
    }

    pub fn is_resuming(&self) -> bool {
        self.resuming.load(Ordering::Relaxed)
    }

    pub fn resuming(&self, is_resuming: bool) {
        self.resuming.store(is_resuming, Ordering::Relaxed);
    }

    pub fn is_not_preparing_to_sleep(&self) -> bool {
        self.preparing_to_sleep.load(Ordering::Relaxed).not()
    }

    pub fn preparing_to_sleep(&self, is_preparing_to_sleep: bool) {
        self.preparing_to_sleep
            .store(is_preparing_to_sleep, Ordering::Relaxed);
    }
}

fn dbus_listener_enabled() -> bool {
    env::var(ENV_DBUS)
        .ok()
        .and_then(|env_dbus| {
            env_dbus
                .parse::<u8>()
                .ok()
                .map(|bb| bb != 0)
                .or_else(|| Some(env_dbus.trim().to_lowercase() != "off"))
        })
        .unwrap_or(true)
}

/// Runs on the sidecar: connects to dbus, then sets the shared flags as `PrepareForSleep` signals
/// arrive. Returns (running deaf) if the connection cannot be established within the timeout.
async fn run_listener(
    run_token: CancellationToken,
    preparing_to_sleep: Arc<AtomicBool>,
    resuming: Arc<AtomicBool>,
) {
    // We wrap the full setup (not just the connect) because the hang has been seen on the AddMatch
    // inside `receive_signal` as well, not only on `Connection::system`.
    // See https://gitlab.com/coolercontrol/coolercontrol/-/issues/264.
    let setup = async {
        let conn = Connection::system().await?;
        let proxy = Proxy::new(
            &conn,
            SLEEP_DEFAULT_BUS_NAME,
            SLEEP_OBJECTPATH,
            SLEEP_INTERFACE,
        )
        .await?;
        let signal = proxy.receive_signal(SIGNAL_PREPARE_FOR_SLEEP).await?;
        Ok::<_, zbus::Error>((conn, signal))
    };
    let (conn, mut sleep_signal) =
        match timeout(Duration::from_secs(DBUS_SETUP_TIMEOUT_S), setup).await {
            Ok(Ok(pair)) => pair,
            Ok(Err(err)) => {
                warn!("Could not connect to DBUS, sleep listener will not work: {err}");
                return;
            }
            Err(_) => {
                warn!(
                    "DBUS sleep listener setup timed out after {DBUS_SETUP_TIMEOUT_S}s; \
                     continuing without it. Sleep/resume events will not be handled on this run."
                );
                return;
            }
        };
    info!("DBUS sleep listener connected.");
    loop {
        tokio::select! {
            () = run_token.cancelled() => break,
            Some(msg) = sleep_signal.next() => {
                // true when entering sleep, false when waking.
                match msg.body().deserialize::<bool>() {
                    Ok(true) => {
                        info!("Received message that system is going to sleep.");
                        preparing_to_sleep.store(true, Ordering::Relaxed);
                    }
                    Ok(false) => {
                        info!("Received message that system is waking from sleep");
                        resuming.store(true, Ordering::Relaxed);
                    }
                    Err(err) => warn!("Failed to read PrepareForSleep signal body: {err}"),
                }
            },
            else => break,
        }
    }
    let _ = conn.close().await;
}
