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

use crate::ENV_DBUS;
use anyhow::Result;
use log::{info, warn};
use moro_local::Scope;
use std::cell::Cell;
use std::env;
use std::ops::Not;
use std::rc::Rc;
use tokio_util::sync::CancellationToken;
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{Connection, Proxy};

pub struct SleepListener {
    preparing_to_sleep: Rc<Cell<bool>>,
    resuming: Rc<Cell<bool>>,
}

impl<'s> SleepListener {
    pub async fn new(
        run_token: CancellationToken,
        scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Result<Self> {
        let listener_enabled = env::var(ENV_DBUS)
            .ok()
            .and_then(|env_dbus| {
                env_dbus
                    .parse::<u8>()
                    .ok()
                    .map(|bb| bb != 0)
                    .or_else(|| Some(env_dbus.trim().to_lowercase() != "off"))
            })
            .unwrap_or(true);
        if listener_enabled.not() {
            info!("DBUS sleep listener disabled.");
            return Ok(Self::create_deaf_listener());
        }
        let conn_result = Connection::system().await;
        if conn_result.is_err() {
            // See issue:
            // https://gitlab.com/coolercontrol/coolercontrol/-/issues/264
            warn!("Could not connect to DBUS, sleep listener will not work!");
            return Ok(Self::create_deaf_listener());
        }
        let conn = conn_result?;
        let proxy = Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;

        let mut sleep_signal = proxy.receive_signal("PrepareForSleep").await?;
        let listener = Self {
            preparing_to_sleep: Rc::new(Cell::new(false)),
            resuming: Rc::new(Cell::new(false)),
        };
        let preparing_to_sleep = Rc::clone(&listener.preparing_to_sleep);
        let resuming = Rc::clone(&listener.resuming);
        scope.spawn(async move {
            loop {
                tokio::select! {
                    () = run_token.cancelled() => break,
                    Some(msg) = sleep_signal.next() => {
                        let body = msg.body();
                        let to_sleep: bool = body.deserialize()?; // returns true if entering sleep, false when waking
                        if to_sleep {
                            info!("Received message that system is going to sleep.");
                            preparing_to_sleep.set(true);
                        } else {
                            info!("Received message that system is waking from sleep");
                            resuming.set(true);
                        }
                    },
                    else => break,
                }
            }
            let _ = conn.close().await;
            Ok::<(), zbus::Error>(())
        });

        Ok(listener)
    }

    fn create_deaf_listener() -> Self {
        Self {
            preparing_to_sleep: Rc::new(Cell::new(false)),
            resuming: Rc::new(Cell::new(false)),
        }
    }

    pub fn is_resuming(&self) -> bool {
        self.resuming.get()
    }

    pub fn resuming(&self, is_resuming: bool) {
        self.resuming.set(is_resuming);
    }

    pub fn is_not_preparing_to_sleep(&self) -> bool {
        self.preparing_to_sleep.get().not()
    }

    pub fn preparing_to_sleep(&self, is_preparing_to_sleep: bool) {
        self.preparing_to_sleep.set(is_preparing_to_sleep);
    }
}
