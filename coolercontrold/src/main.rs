/* CoolerControl - monitor and control your cooling and other devices
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
use std::ops::Not;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use clokwerk::{AsyncScheduler, Interval};
use log::{debug, error, info, LevelFilter};
use nix::unistd::Uid;
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;
use tokio::time::Instant;
use zbus::export::futures_util::future::join_all;

use repositories::repository::Repository;

use crate::config::Config;
use crate::device::{Device, DeviceType, UID};
use crate::device_commander::DeviceCommander;
use crate::repositories::composite_repo::CompositeRepo;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu_repo::GpuRepo;
use crate::repositories::hwmon::hwmon_repo::HwmonRepo;
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::repository::{DeviceList, DeviceLock};
use crate::sleep_listener::SleepListener;

mod repositories;
mod device;
mod setting;
mod gui_server;
mod device_commander;
mod config;
mod speed_scheduler;
mod utils;
mod sleep_listener;
mod lcd_scheduler;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

type Repos = Arc<Vec<Arc<dyn Repository>>>;
type AllDevices = Arc<HashMap<UID, DeviceLock>>;

/// A program to control your cooling devices
#[derive(Parser, Debug)]
#[clap(author, about, long_about = None)]
struct Args {
    /// Enable debug output
    #[clap(long)]
    debug: bool,

    /// Get current version info
    #[clap(long, short)]
    version: bool,

    /// Check config file validity
    #[clap(long)]
    config: bool,
}

/// Main Control Loop
#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    info!("Initializing...");
    let term_signal = setup_term_signal()?;
    if !Uid::effective().is_root() {
        return Err(anyhow!("coolercontrold must be run with root permissions"));
    }
    let config = Arc::new(Config::load_config_file().await?);
    if Args::parse().config {
        std::process::exit(0);
    }
    if let Err(err) = config.save_config_file().await {
        return Err(err);
    }
    let mut scheduler = AsyncScheduler::new();

    tokio::time::sleep( // some hardware needs more time to startup before we can communicate
                        config.get_settings().await?.startup_delay
    ).await;
    let mut init_repos: Vec<Arc<dyn Repository>> = vec![];
    match init_liquidctl_repo(config.clone()).await { // should be first as it's the slowest
        Ok(repo) => init_repos.push(Arc::new(repo)),
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

    if config.get_settings().await?.apply_on_boot {
        apply_saved_device_settings(&config, &all_devices, &device_commander).await;
    }

    let sleep_listener = SleepListener::new().await
        .with_context(|| "Creating DBus Sleep Listener")?;

    let server = gui_server::init_server(
        all_devices.clone(), device_commander.clone(), config.clone(),
    ).await?;
    tokio::task::spawn(server);

    add_update_job_to_scheduler(&mut scheduler, &repos, &device_commander);
    add_lcd_update_job_to_scheduler(&mut scheduler, &device_commander);

    // main loop:
    while !term_signal.load(Ordering::Relaxed) {
        if sleep_listener.is_waking_up() {
            // delay at least a second to allow the hardware to fully wake up:
            tokio::time::sleep(
                config.get_settings().await?.startup_delay
                    .max(Duration::from_secs(1))
            ).await;
            if config.get_settings().await?.apply_on_boot {
                info!("Re-initializing and re-applying settings after waking from sleep");
                device_commander.reinitialize_devices().await;
                apply_saved_device_settings(&config, &all_devices, &device_commander).await;
            }
            sleep_listener.waking_up(false);
            sleep_listener.sleeping(false);
        } else if sleep_listener.is_sleeping().not() {
            scheduler.run_pending().await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    shutdown(repos).await
}

fn setup_logging() {
    let version = VERSION.unwrap_or("unknown");
    let args = Args::parse();
    if args.debug {
        log::set_max_level(LevelFilter::Debug);
    } else if let Ok(log_lvl) = std::env::var("COOLERCONTROL_LOG") {
        info!("Logging Level set to {}", log_lvl);
        let level = match LevelFilter::from_str(&log_lvl) {
            Ok(lvl) => lvl,
            Err(_) => LevelFilter::Info
        };
        log::set_max_level(level);
    } else {
        log::set_max_level(LevelFilter::Info);
    }
    let timestamp_precision = if args.debug {
        env_logger::fmt::TimestampPrecision::Millis
    } else {
        env_logger::fmt::TimestampPrecision::Seconds
    };
    if connected_to_journal() {
        systemd_journal_logger::init_with_extra_fields(
            vec![("VERSION", version)]
        ).unwrap();
    } else {
        env_logger::builder()
            .filter_level(log::max_level())
            .format_timestamp(Some(timestamp_precision))
            .format_timestamp_millis()
            .init();
    }
    debug!("Debug output enabled");
    if log::max_level() == LevelFilter::Debug || args.version {
        let sys = System::new();
        info!("\n\
            CoolerControlD v{}\n\n\
            System:\n\
            \t{}\n\
            \t{}\n\
            ",
            version,
            sys.long_os_version().unwrap_or_default(),
            sys.kernel_version().unwrap_or_default(),
        );
    }
    if args.version {
        std::process::exit(0);
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
    lc_repo.initialize_devices().await?;
    lc_repo.preload_statuses().await;
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

async fn apply_saved_device_settings(
    config: &Arc<Config>,
    all_devices: &AllDevices,
    device_commander: &Arc<DeviceCommander>,
) {
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
}

fn add_update_job_to_scheduler(
    scheduler: &mut AsyncScheduler,
    repos: &Repos,
    device_commander: &Arc<DeviceCommander>,
) {
    let pass_repos = Arc::clone(&repos);
    let pass_speed_scheduler = Arc::clone(&device_commander.speed_scheduler);

    scheduler.every(Interval::Seconds(1))
        .run(
            move || {
                // we need to pass the references in twice
                let moved_repos = Arc::clone(&pass_repos);
                let moved_speed_scheduler = Arc::clone(&pass_speed_scheduler);
                Box::pin({
                    async move {
                        debug!("Status updates triggered");
                        let start_initialization = Instant::now();
                        let mut futures = Vec::new();
                        for repo in moved_repos.iter() {
                            futures.push(repo.preload_statuses())
                        }
                        join_all(futures).await;
                        let mut futures = Vec::new();
                        for repo in moved_repos.iter() {
                            if repo.device_type() != DeviceType::Composite {
                                futures.push(repo.update_statuses())
                            }
                        }
                        for result in join_all(futures).await.iter() {
                            if let Err(err) = result {
                                error!("Error trying to update statuses: {}", err)
                            }
                        }
                        // composite repos should be updated after all real devices
                        for repo in moved_repos.iter() {
                            if repo.device_type() == DeviceType::Composite {
                                if let Err(err) = repo.update_statuses().await {
                                    error!("Error trying to update statuses: {}", err)
                                }
                            }
                        }
                        debug!("Time taken to update all devices: {:?}", start_initialization.elapsed());
                        debug!("Speed Scheduler triggered");
                        // NOTE: Schedulers not dependant on the current status of the device
                        //   should be in their own job (don't block the update job)
                        moved_speed_scheduler.update_speed().await;
                    }
                })
            }
        );
}

fn add_lcd_update_job_to_scheduler(
    scheduler: &mut AsyncScheduler,
    device_commander: &Arc<DeviceCommander>,
) {
    let pass_lcd_scheduler = Arc::clone(&device_commander.lcd_scheduler);
    scheduler.every(Interval::Seconds(1))
        .run(
            move || {
                // we need to pass the references in twice
                let moved_lcd_scheduler = Arc::clone(&pass_lcd_scheduler);
                Box::pin({
                    async move {
                        debug!("LCD Scheduler triggered");
                        moved_lcd_scheduler.update_lcd().await;
                    }
                })
            }
        );
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
