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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use clokwerk::{ScheduleHandle, Scheduler, TimeUnits};
use clokwerk::Interval::*;
use flume::{Receiver, Sender};
use log::error;

use crate::{LiquidctlRepo, MainMessage, Repository};
use crate::liquidctl::liquidctl_repo::ClientMessage;

/// This service with update all the devices' statuses in the background at a specified interval
pub struct StatusUpdater {
    pub thread: ScheduleHandle,
}

impl StatusUpdater {
    pub fn new(tx_to_main: Sender<MainMessage>, rx_from_main: Receiver<MainMessage>) -> Self {
        let mut scheduler = Scheduler::new();
        scheduler
            .every(1.seconds())
            .run(move || {
                tx_to_main.send(MainMessage::UpdateStatuses).unwrap_or_else(
                    |err| error!("Error sending message to Main: {}", err)
                );
                match rx_from_main.recv() {
                    Ok(msg) => {
                        if msg != MainMessage::UpdateStatuses {
                            error!("Unexpected Response from Main for Status Update: {:?}", msg)
                        }
                    }
                    Err(err) => error!("Error waiting on message from Main: {}", err)
                };
            });
        let thread = scheduler.watch_thread(Duration::from_millis(100));
        Self { thread }
    }
}