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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use log::info;
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{Connection, Proxy};

pub struct SleepListener {
    sleeping: Arc<AtomicBool>,
    waking_up: Arc<AtomicBool>,
}

impl SleepListener {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .with_context(|| "Connecting to DBUS. If this errors out DBus might not be running")?;
        let proxy = Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;

        let mut sleep_signal = proxy.receive_signal("PrepareForSleep").await?;
        let sleeping = Arc::new(AtomicBool::new(false));
        let waking_up = Arc::new(AtomicBool::new(false));

        let cloned_sleeping = Arc::clone(&sleeping);
        let cloned_waking_up = Arc::clone(&waking_up);
        tokio::spawn(async move {
            while let Some(msg) = sleep_signal.next().await {
                let body = msg.body();
                let to_sleep: bool = body.deserialize()?; // returns true if entering sleep, false when waking
                if to_sleep {
                    info!("System is going to sleep");
                    cloned_sleeping.store(true, Ordering::SeqCst);
                } else {
                    info!("System is waking from sleep");
                    cloned_waking_up.store(true, Ordering::SeqCst);
                }
            }
            Ok::<(), zbus::Error>(())
        });
        Ok(Self {
            sleeping,
            waking_up,
        })
    }

    pub fn is_waking_up(&self) -> bool {
        self.waking_up.load(Ordering::Relaxed)
    }

    pub fn waking_up(&self, is: bool) {
        self.waking_up.store(is, Ordering::SeqCst);
    }

    pub fn is_sleeping(&self) -> bool {
        self.sleeping.load(Ordering::Relaxed)
    }

    pub fn sleeping(&self, is: bool) {
        self.sleeping.store(is, Ordering::SeqCst);
    }
}
