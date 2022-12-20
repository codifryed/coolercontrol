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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use log::info;
use zbus::{Connection, Proxy};
use zbus::export::ordered_stream::OrderedStreamExt;

pub struct SleepListener {
    pub going_to_sleep: Arc<AtomicBool>,
    pub waking_up: Arc<AtomicBool>,
}

impl SleepListener {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system().await?;
        let proxy = Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        ).await?;

        let mut sleep_signal = proxy.receive_signal("PrepareForSleep").await?;
        let going_to_sleep = Arc::new(AtomicBool::new(false));
        let waking_up = Arc::new(AtomicBool::new(false));

        let cloned_going_to_sleep = Arc::clone(&going_to_sleep);
        let cloned_waking_up = Arc::clone(&waking_up);
        tokio::spawn(
            async move {
                while let Some(sig) = sleep_signal.next().await {
                    let to_sleep: bool = sig.body()?; // returns true if entering sleep, false when waking
                    if to_sleep {
                        info!("System is going to sleep");
                        cloned_going_to_sleep.store(true, Ordering::SeqCst);
                    } else {
                        info!("System is waking from sleep");
                        cloned_waking_up.store(true, Ordering::SeqCst);
                    }
                }
                Ok::<(), zbus::Error>(())
            }
        );
        Ok(Self {
            going_to_sleep,
            waking_up,
        })
    }
}