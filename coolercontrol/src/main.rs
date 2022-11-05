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
use clap::Parser;
use log::{debug, error, info, LevelFilter};
use serde::{Deserialize, Serialize};
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use simple_logger::SimpleLogger;
use strum::{Display, EnumString};
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;
use tokio::sync::RwLock;

use crate::device::Device;
use crate::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repository::Repository;
use crate::status_updater::StatusUpdater;

mod liquidctl;
mod device;
mod repository;
mod setting;
mod status_updater;
mod gui_server;


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
#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    let term_signal = setup_term_signal()?;

    let repos: Arc<RwLock<Vec<Box<dyn Repository>>>> = Arc::new(RwLock::new(vec![]));
    match init_liquidctl_repo().await {
        Ok(repo) => repos.write().await.push(Box::new(repo)),
        Err(err) => error!("Error initializing Liquidctl Repo: {}", err)
    };

    let server = gui_server::init_server(repos.clone()).await?;
    tokio::task::spawn(server);


    // This is used for status updates... perhaps we want to schedule other things?
    //  Things to schedule/send/Jobs:
    //  - Status Updates
    //  - incoming requests from the UI (have to see how to build this)
    //  - Scheduled settings -> speed / color / lcd
    //    - SettingsJob with: device type, device_id, settings to set
    //    - perhaps we have a SettingsScheduler that hold jobs internally, checking if/when to
    //      send messages to the main thread here. (needs to read device status)
    //  - DeviceCommander -> to take sent settings from the UI and convert & assign them to
    //    specific repos
    //
    let (
        tx_from_updater_to_main,
        rx_from_updater_to_main
    ) = flume::unbounded();
    let status_updater = StatusUpdater::new(tx_from_updater_to_main);


    // ASYNC & ClockWork version:
    while !term_signal.load(Ordering::Relaxed) {
        if let Ok(msg) = rx_from_updater_to_main.recv_async().await {
            match msg {
                MainMessage::UpdateStatuses => {
                    debug!("Status updates triggered");
                    for repo in repos.read().await.iter() {
                        if let Err(err) = repo.update_statuses().await {
                            error!("Error trying to update statuses: {}", err)
                        }
                    }
                }
            }
        }
    }

    shutdown(repos, status_updater).await
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

async fn init_liquidctl_repo() -> Result<LiquidctlRepo> {
    let mut lc_repo = LiquidctlRepo::new().await?;
    lc_repo.get_devices().await?;
    lc_repo.connect_devices().await?;
    lc_repo.initialize_devices().await?;
    lc_repo.update_statuses().await?;
    Ok(lc_repo)
}

async fn shutdown(
    repos: Arc<RwLock<Vec<Box<dyn Repository>>>>,
    status_updater: StatusUpdater,
) -> Result<()> {
    info!("Main process shutting down");
    status_updater.thread.stop();
    for repo in repos.read().await.iter() {
        match repo.shutdown().await {
            Ok(_) => {}
            Err(err) => error!("Shutdown error: {}", err)
        };
    }
    Ok(())
}

// todo: we can probably move this to StatusUpdater
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum MainMessage {
    UpdateStatuses,
}