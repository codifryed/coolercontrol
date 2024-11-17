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

use anyhow::{Context, Result};
use log::info;
use moro_local::Scope;
use std::cell::Cell;
use std::ops::Not;
use std::rc::Rc;
use zbus::export::ordered_stream::OrderedStreamExt;
use zbus::{Connection, Proxy};

pub struct SleepListener {
    preparing_to_sleep: Rc<Cell<bool>>,
    resuming: Rc<Cell<bool>>,
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
        let listener = Self {
            preparing_to_sleep: Rc::new(Cell::new(false)),
            resuming: Rc::new(Cell::new(false)),
        };
        let preparing_to_sleep = Rc::clone(&listener.preparing_to_sleep);
        let resuming = Rc::clone(&listener.resuming);
        scope.spawn(async move {
            while let Some(msg) = sleep_signal.next().await {
                let body = msg.body();
                let to_sleep: bool = body.deserialize()?; // returns true if entering sleep, false when waking
                if to_sleep {
                    info!("System is going to sleep");
                    preparing_to_sleep.set(true);
                } else {
                    info!("System is waking from sleep");
                    resuming.set(true);
                }
            }
            Ok::<(), zbus::Error>(())
        });

        Ok(listener)
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
