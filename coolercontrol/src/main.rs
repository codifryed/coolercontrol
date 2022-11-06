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
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use simple_logger::SimpleLogger;
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;
use tokio::sync::RwLock;

use repositories::repository::Repository;

use crate::device::Device;
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu_repo::GpuRepo;
use crate::repositories::liquidctl::liqctld_client::LiqctldUpdateClient;
use crate::status_updater::{SchedulerMessage, StatusUpdater};

mod repositories;
mod device;
mod setting;
mod status_updater;
mod gui_server;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

type Repos = Arc<RwLock<Vec<Box<dyn Repository>>>>;

/// A program to control your cooling devices
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Enable debug output
    #[clap(long)]
    debug: bool,
}

/// Main Control Loop
#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    let term_signal = setup_term_signal()?;

    // We use the tokio::RwLock here because it does not have the write starvation issue
    //  that the std version does, which could be a problem for us if enough clients requests come in.
    let repos: Repos = Arc::new(RwLock::new(vec![]));
    let mut liquidctl_update_client: Option<Arc<LiqctldUpdateClient>> = None;
    match init_liquidctl_repo().await { // should be first as it's the slowest
        Ok(repo) => {
            liquidctl_update_client = Some(repo.liqctld_update_client.clone());
            repos.write().await.push(Box::new(repo))
        }
        Err(err) => error!("Error initializing Liquidctl Repo: {}", err)
    };
    match init_cpu_repo().await {
        Ok(repo) => repos.write().await.push(Box::new(repo)),
        Err(err) => error!("Error initializing CPU Repo: {}", err)
    }
    match init_gpu_repo().await {
        Ok(repo) => repos.write().await.push(Box::new(repo)),
        Err(err) => error!("Error initializing GPU Repo: {}", err)
    }


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
                SchedulerMessage::UpdateStatuses => {
                    debug!("Status updates triggered");
                    if let Some(ref update_client) = liquidctl_update_client {
                        update_client.preload_statuses().await
                    }
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
    let version = VERSION.unwrap_or("unknown");
    if connected_to_journal() {
        systemd_journal_logger::init_with_extra_fields(
            vec![("VERSION", version)]
        ).unwrap();
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
        debug!("\n\
            CoolerControl v{}\n\

            System:
  {}
  {}\n\
            ",
            version,
            sys.long_os_version().unwrap_or_default(),
            sys.kernel_version().unwrap_or_default(),
        );
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
    lc_repo.liqctld_update_client.preload_statuses().await;
    lc_repo.update_statuses().await?;
    Ok(lc_repo)
}

async fn init_cpu_repo() -> Result<CpuRepo> {
    let cpu_repo = CpuRepo::new().await?;
    cpu_repo.initialize_devices().await?;
    Ok(cpu_repo)
}

async fn init_gpu_repo() -> Result<GpuRepo> {
    let gpu_repo = GpuRepo::new().await?;
    gpu_repo.initialize_devices().await?;
    Ok(gpu_repo)
}

async fn shutdown(
    repos: Repos,
    status_updater: StatusUpdater,
) -> Result<()> {
    info!("Main process shutting down");
    status_updater.thread.stop();
    for repo in repos.read().await.iter() {
        if let Err(err) = repo.shutdown().await {
            error!("Shutdown error: {}", err)
        };
    }
    info!("Shutdown Complete");
    Ok(())
}
