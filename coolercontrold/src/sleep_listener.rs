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
use moro_local::Scope;
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{Connection, Proxy};

pub struct SleepListener {
    preparing_to_sleep: Arc<AtomicBool>,
    resuming: Arc<AtomicBool>,
}

impl<'s> SleepListener {
    pub async fn new(scope: &'s Scope<'s, 's, Result<()>>) -> Result<Self> {
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
        let preparing_to_sleep = Arc::new(AtomicBool::new(false));
        let resuming = Arc::new(AtomicBool::new(false));

        let cloned_going_to_sleep = Arc::clone(&preparing_to_sleep);
        let cloned_resuming = Arc::clone(&resuming);
        scope.spawn(async move {
            while let Some(msg) = sleep_signal.next().await {
                let body = msg.body();
                let to_sleep: bool = body.deserialize()?; // returns true if entering sleep, false when waking
                if to_sleep {
                    info!("System is going to sleep");
                    cloned_going_to_sleep.store(true, Ordering::SeqCst);
                } else {
                    info!("System is waking from sleep");
                    cloned_resuming.store(true, Ordering::SeqCst);
                }
            }
            Ok::<(), zbus::Error>(())
        });
        Ok(Self {
            preparing_to_sleep,
            resuming,
        })
    }

    pub fn is_resuming(&self) -> bool {
        self.resuming.load(Ordering::Relaxed)
    }

    pub fn resuming(&self, is_resuming: bool) {
        self.resuming.store(is_resuming, Ordering::SeqCst);
    }

    pub fn is_preparing_to_sleep(&self) -> bool {
        self.preparing_to_sleep.load(Ordering::Relaxed)
    }

    pub fn preparing_to_sleep(&self, is_preparing_to_sleep: bool) {
        self.preparing_to_sleep
            .store(is_preparing_to_sleep, Ordering::SeqCst);
    }
}
