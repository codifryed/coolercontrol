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

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use log::{debug, error, info, LevelFilter};
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;
use tokio::time::Instant;
use tokio_cron_scheduler::{Job, JobScheduler};

use repositories::repository::Repository;

use crate::config::Config;
use crate::device::{Device, UID};
use crate::device_commander::DeviceCommander;
use crate::repositories::composite_repo::CompositeRepo;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu_repo::GpuRepo;
use crate::repositories::hwmon::hwmon_repo::HwmonRepo;
use crate::repositories::liquidctl::liqctld_client::LiqctldUpdateClient;
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::repository::{DeviceList, DeviceLock};

mod repositories;
mod device;
mod setting;
mod gui_server;
mod device_commander;
mod config;
mod speed_scheduler;
mod utils;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

type Repos = Arc<Vec<Arc<dyn Repository>>>;
type AllDevices = Arc<HashMap<UID, DeviceLock>>;

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
    let config = Arc::new(Config::load_config_file().await?);
    let scheduler = JobScheduler::new().await?;
    // todo: also check if the gui-server-port is free for use, if not shutdown as well

    let mut init_repos: Vec<Arc<dyn Repository>> = vec![];
    let mut liquidctl_update_client: Option<Arc<LiqctldUpdateClient>> = None;
    match init_liquidctl_repo(config.clone()).await { // should be first as it's the slowest
        Ok(repo) => {
            liquidctl_update_client = Some(repo.liqctld_update_client.clone());
            init_repos.push(Arc::new(repo))
        }
        Err(err) => error!("Error initializing Liquidctl Repo: {}", err)
    };
    match init_cpu_repo().await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing CPU Repo: {}", err)
    }
    match init_gpu_repo().await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing GPU Repo: {}", err)
    }
    match init_hwmon_repo().await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing Hwmon Repo: {}", err)
    }
    let devices_for_composite = collect_devices_for_composite(&init_repos).await;
    match init_composite_repo(devices_for_composite).await {  // should be last as it uses all other device temps
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing Composite Repo: {}", err)
    }
    let repos: Repos = Arc::new(init_repos);

    let mut all_devices = HashMap::new();
    for repo in repos.iter() {
        for device_lock in repo.devices().await {
            let uid = device_lock.read().await.uid.clone();
            all_devices.insert(
                uid,
                Arc::clone(&device_lock),
            );
        }
    }
    let all_devices: AllDevices = Arc::new(all_devices);
    config.create_device_list(all_devices.clone()).await?;
    config.save_config_file().await?;
    let device_commander = Arc::new(DeviceCommander::new(
        all_devices.clone(),
        repos.clone(),
        config.clone(),
    ));

    info!("Applying saved device settings");
    for uid in all_devices.keys() {
        match config.get_device_settings(uid).await {
            Ok(settings) => {
                debug!("Settings for device: {} loaded from config file: {:?}", uid, settings);
                for setting in settings.iter() {
                    if let Err(err) = device_commander.set_setting(uid, setting).await {
                        error!("Error setting device setting: {}", err);
                    }
                }
            }
            Err(err) => error!("Error trying to read device settings from config file: {}", err)
        }
    }

    let server = gui_server::init_server(
        all_devices.clone(), device_commander, config.clone(),
    ).await?;
    tokio::task::spawn(server);

    // todo: dbus sleep watcher

    // Scheduled Updates:
    let pass_repos = Arc::clone(&repos);
    let pass_liq_client = if let Some(client) = liquidctl_update_client {
        Some(Arc::clone(&client))
    } else {
        None
    };
    scheduler.add(Job::new_repeated_async(
        Duration::from_millis(1000),
        move |_uuid, _l| {
            // we need to pass the references in twice
            let moved_repos = Arc::clone(&pass_repos);
            let moved_liq_client = if let Some(client) = &pass_liq_client {
                Some(Arc::clone(&client))
            } else {
                None
            };
            Box::pin({
                async move {
                    info!("Status updates triggered");
                    let start_initialization = Instant::now();
                    if let Some(ref update_client) = moved_liq_client {
                        update_client.preload_statuses().await
                    }
                    for repo in moved_repos.iter() {
                        if let Err(err) = repo.update_statuses().await {
                            error!("Error trying to update statuses: {}", err)
                        }
                    }
                    debug!("Time taken to update all devices: {:?}", start_initialization.elapsed());
                }
            })
        }).unwrap()).await?;

    // main loop:
    while !term_signal.load(Ordering::Relaxed) {
        scheduler.tick().await?;
        // 999 has the best results (time_to_next_job and 1 sec both have occasional issues)
        tokio::time::sleep(Duration::from_millis(999)).await;
    }

    shutdown(repos).await
}

fn setup_logging() {
    let version = VERSION.unwrap_or("unknown");
    let args = Args::parse();
    log::set_max_level(
        if args.debug { LevelFilter::Debug } else { LevelFilter::Info }
    );
    if connected_to_journal() {
        systemd_journal_logger::init_with_extra_fields(
            vec![("VERSION", version)]
        ).unwrap();
    } else {
        env_logger::builder().filter_level(log::max_level()).init();
    }
    info!("Initializing...");
    debug!("Debug output enabled");
    if log::max_level() == LevelFilter::Debug {
        let sys = System::new();
        debug!("\n\
            CoolerControl v{}\n\n\
            System:\n\
            \t{}\n\
            \t{}\n\
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

async fn init_liquidctl_repo(config: Arc<Config>) -> Result<LiquidctlRepo> {
    let mut lc_repo = LiquidctlRepo::new(config).await?;
    lc_repo.get_devices().await?;
    lc_repo.connect_devices().await?;
    lc_repo.initialize_devices().await?;
    lc_repo.liqctld_update_client.preload_statuses().await;
    lc_repo.update_statuses().await?;
    Ok(lc_repo)
}

async fn init_cpu_repo() -> Result<CpuRepo> {
    let mut cpu_repo = CpuRepo::new().await?;
    cpu_repo.initialize_devices().await?;
    Ok(cpu_repo)
}

async fn init_gpu_repo() -> Result<GpuRepo> {
    let mut gpu_repo = GpuRepo::new().await?;
    gpu_repo.initialize_devices().await?;
    Ok(gpu_repo)
}

async fn init_hwmon_repo() -> Result<HwmonRepo> {
    let mut hwmon_repo = HwmonRepo::new().await?;
    hwmon_repo.initialize_devices().await?;
    Ok(hwmon_repo)
}

async fn init_composite_repo(devices_for_composite: DeviceList) -> Result<CompositeRepo> {
    let mut composite_repo = CompositeRepo::new(devices_for_composite);
    composite_repo.initialize_devices().await?;
    Ok(composite_repo)
}

/// Create separate list of devices to be used in the composite repository
async fn collect_devices_for_composite(init_repos: &[Arc<dyn Repository>]) -> DeviceList {
    let mut devices_for_composite = Vec::new();
    for repo in init_repos.iter() {
        for device_lock in repo.devices().await {
            devices_for_composite.push(Arc::clone(&device_lock));
        }
    }
    devices_for_composite
}

async fn shutdown(repos: Repos) -> Result<()> {
    info!("Main process shutting down");
    for repo in repos.iter() {
        if let Err(err) = repo.shutdown().await {
            error!("Shutdown error: {}", err)
        };
    }
    info!("Shutdown Complete");
    Ok(())
}
