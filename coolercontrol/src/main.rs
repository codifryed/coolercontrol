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
use std::time::Duration;

use anyhow::{bail, Result};
use clap::Parser;
use log::{debug, error, info, LevelFilter};
use serde::{Deserialize, Serialize};
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use simple_logger::SimpleLogger;
use strum::{Display, EnumString};
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;

use crate::device::Device;
use crate::liqctld_client::Client;
use crate::liquidctl::liqctld_client;
use crate::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repository::Repository;
use crate::status_updater::StatusUpdater;

mod liquidctl;
mod device;
mod repository;
mod setting;
mod status_updater;

const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// A program to control your cooling devices
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Enable debug output
    #[clap(long)]
    debug: bool,
}

/// Main Control Loop
///  start main engine ->
///   listening to two channels
///     one from the scheduler thread
///     one from the UI interface thread
///   will control all the main methods & services
fn main() -> Result<()> {
    setup_logging();
    let term_signal = setup_term_signal()?;

    let liquidctl_repo = init_liquidctl_repo()?;
    let repos = vec![
        &liquidctl_repo,
    ];

    let (
        tx_from_main_to_updater,
        rx_from_main_to_updater
    ) = flume::unbounded();
    let (
        tx_from_updater_to_main,
        rx_from_updater_to_main
    ) = flume::unbounded();
    // let (
    //     tx_from_main_to_ui,
    //     rx_from_main_to_ui
    // ) = flume::unbounded();
    // let (
    //     tx_from_ui_to_main,
    //     rx_from_ui_to_main
    // ) = flume::unbounded();

    let status_updater = StatusUpdater::new(
        tx_from_updater_to_main,
        rx_from_main_to_updater,
    );
    // todo: start_UI_listener

    while !term_signal.load(Ordering::Relaxed) {
        // let rx_from_updater = rx_from_updater_to_main.to_owned();
        // let tx_to_updater = tx_from_main_to_updater.to_owned();
        // let rx_from_ui = rx_from_ui_to_main;
        // let tx_to_ui = tx_from_main_to_ui;
        if let Ok(msg) = rx_from_updater_to_main.recv_timeout(RECV_TIMEOUT) {
            match msg {
                MainMessage::UpdateStatuses => {
                    liquidctl_repo.update_statuses();
                    tx_from_main_to_updater.send(MainMessage::UpdateStatuses).unwrap_or_else(
                        |err| error!("Error sending message to Updater: {}", err)
                    )
                }
                // _ => error!("No implementation yet for {}", msg)
            }
        }
        // else if Some(msg) = rx_from_ui.recv_timeout(wait_time) {
        //
        // }
    }
    shutdown(status_updater, repos)
}

fn setup_logging() {
    if connected_to_journal() {
        systemd_journal_logger::init_with_extra_fields(
            vec![("VERSION", env!("CARGO_PKG_VERSION"))]).unwrap();
    } else {
        SimpleLogger::new().init().unwrap();
    }
    let args = Args::parse();
    log::set_max_level(
        if args.debug { LevelFilter::Debug } else { LevelFilter::Info }
    );
    info!("Initializing...");
    debug!("Debug output enabled");
    if log::max_level() == LevelFilter::Debug {
        let sys = System::new();
        debug!("System Info:");
        debug!("    OS: {}", sys.long_os_version().unwrap_or_default());
        debug!("    Kernel: {}", sys.kernel_version().unwrap_or_default());
    }
}

fn setup_term_signal() -> Result<Arc<AtomicBool>> {
    let term_signal = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGTERM, Arc::clone(&term_signal))?;
    signal_hook::flag::register(SIGINT, Arc::clone(&term_signal))?;
    signal_hook::flag::register(SIGQUIT, Arc::clone(&term_signal))?;
    Ok(term_signal)
}

fn init_liquidctl_repo() -> Result<LiquidctlRepo> {
    let liquidctl_repo = match LiquidctlRepo::new() {
        Ok(repo) => repo,
        Err(err) => bail!("Could instantiate the Liquidctl Repository: {}", err)
    };
    liquidctl_repo.initialize_devices();
    Ok(liquidctl_repo)
}

fn shutdown(status_updater: StatusUpdater, repos: Vec<&LiquidctlRepo>) -> Result<()> {
    info!("Shutting down");
    status_updater.thread.stop();
    for repo in repos {
        repo.shutdown();
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum MainMessage {
    UpdateStatuses,
}