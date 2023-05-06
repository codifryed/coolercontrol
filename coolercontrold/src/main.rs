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
use tokio::time::sleep;
use tokio::time::Instant;

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
mod thinkpad_utils;

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

    sleep( // some hardware needs more time to startup before we can communicate
           config.get_settings().await?.startup_delay
    ).await;
    let mut init_repos: Vec<Arc<dyn Repository>> = vec![];
    match init_liquidctl_repo(config.clone()).await { // should be first as it's the slowest
        Ok(repo) => init_repos.push(repo),
        Err(err) => error!("Error initializing LIQUIDCTL Repo: {}", err)
    };
    match init_cpu_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing CPU Repo: {}", err)
    }
    match init_gpu_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing GPU Repo: {}", err)
    }
    match init_hwmon_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing HWMON Repo: {}", err)
    }
    let devices_for_composite = collect_devices_for_composite(&init_repos).await;
    match init_composite_repo(config.clone(), devices_for_composite).await {  // should be last as it uses all other device temps
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing COMPOSITE Repo: {}", err)
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

    add_preload_jobs_into(&mut scheduler, &repos);
    add_status_snapshot_job_into(&mut scheduler, &repos, &device_commander);
    add_lcd_update_job_into(&mut scheduler, &device_commander);

    info!("Daemon successfully initialized");
    // main loop:
    while !term_signal.load(Ordering::Relaxed) {
        if sleep_listener.is_waking_up() {
            // delay at least a second to allow the hardware to fully wake up:
            sleep(
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
            // this await will block future jobs if one of the scheduled jobs is long-running:
            scheduler.run_pending().await;
        }
        sleep(Duration::from_millis(100)).await;
    }
    sleep(Duration::from_millis(500)).await; // wait for already scheduled jobs to finish
    shutdown(repos).await
}

fn setup_logging() {
    let version = VERSION.unwrap_or("unknown");
    let args = Args::parse();
    let log_level =
        if args.debug {
            LevelFilter::Debug
        } else if let Ok(log_lvl) = std::env::var("COOLERCONTROL_LOG") {
            match LevelFilter::from_str(&log_lvl) {
                Ok(lvl) => lvl,
                Err(_) => LevelFilter::Info
            }
        } else {
            LevelFilter::Info
        };
    log::set_max_level(log_level);
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
    info!("Logging Level: {}", log::max_level());
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

async fn init_liquidctl_repo(config: Arc<Config>) -> Result<Arc<LiquidctlRepo>> {
    let mut lc_repo = LiquidctlRepo::new(config).await?;
    lc_repo.get_devices().await?;
    lc_repo.initialize_devices().await?;
    let lc_repo = Arc::new(lc_repo);
    Arc::clone(&lc_repo).preload_statuses().await;
    lc_repo.update_statuses().await?;
    Ok(lc_repo)
}

async fn init_cpu_repo(config: Arc<Config>) -> Result<CpuRepo> {
    let mut cpu_repo = CpuRepo::new(config).await?;
    cpu_repo.initialize_devices().await?;
    Ok(cpu_repo)
}

async fn init_gpu_repo(config: Arc<Config>) -> Result<GpuRepo> {
    let mut gpu_repo = GpuRepo::new(config).await?;
    gpu_repo.initialize_devices().await?;
    Ok(gpu_repo)
}

async fn init_hwmon_repo(config: Arc<Config>) -> Result<HwmonRepo> {
    let mut hwmon_repo = HwmonRepo::new(config).await?;
    hwmon_repo.initialize_devices().await?;
    Ok(hwmon_repo)
}

async fn init_composite_repo(config: Arc<Config>, devices_for_composite: DeviceList) -> Result<CompositeRepo> {
    let mut composite_repo = CompositeRepo::new(config, devices_for_composite);
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

/// This Job will run the status preload task for every repository individually.
/// This allows each repository to handle it's own timings for it's devices and be independent
/// of the status snapshots that will happen irregardless every second.
fn add_preload_jobs_into(scheduler: &mut AsyncScheduler, repos: &Repos) {
    for repo in repos.iter() {
        if repo.device_type() == DeviceType::Composite {
            continue; // Composite repos don't preload statuses
        }
        let pass_repo = Arc::clone(&repo);
        scheduler.every(Interval::Seconds(1))
            .run(
                move || {
                    let moved_repo = Arc::clone(&pass_repo);
                    Box::pin(async move {
                        tokio::task::spawn(async move {
                            debug!("STATUS PRELOAD triggered for {} repo", moved_repo.device_type());
                            moved_repo.preload_statuses().await;
                        });
                    })
                }
            );
    }
}

/// This job should snapshot the status of each device in each repository as it is now.
/// This allows us to have a steady stream of status updates consistently every second,
/// regardless of if some device is particularly slow for a while or not.
fn add_status_snapshot_job_into(
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
                Box::pin(async move {
                    // sleep used to attempt to place the jobs appropriately in time
                    // as they tick off at the same time per second.
                    sleep(Duration::from_millis(400)).await;
                    debug!("STATUS SNAPSHOTS triggered");
                    let start_initialization = Instant::now();
                    for repo in moved_repos.iter() {
                        // composite repos should be updated after all real devices
                        //  so they should definitely be last in the list
                        if let Err(err) = repo.update_statuses().await {
                            error!("Error trying to update status: {}", err)
                        }
                    }
                    debug!("STATUS SNAPSHOT Time taken for all devices: {:?}", start_initialization.elapsed());
                    moved_speed_scheduler.update_speed().await;
                })
            }
        );
}

/// The LCD Update job that often takes a long time (>1.0s, <2.0s). It runs in it's own thread to
/// not affect the other jobs in the main loop, but will also timeout to keep very long running
/// jobs from pilling up.
fn add_lcd_update_job_into(
    scheduler: &mut AsyncScheduler,
    device_commander: &Arc<DeviceCommander>,
) {
    let pass_lcd_scheduler = Arc::clone(&device_commander.lcd_scheduler);
    let lcd_update_interval = 2_u32;
    scheduler.every(Interval::Seconds(lcd_update_interval))
        .run(
            move || {
                // we need to pass the references in twice
                let moved_lcd_scheduler = Arc::clone(&pass_lcd_scheduler);
                Box::pin(async move {
                    // sleep used to attempt to place the jobs appropriately in time
                    // as they tick off at the same time per second.
                    sleep(Duration::from_millis(500)).await;
                    tokio::task::spawn(async move {
                        if let Err(_) = tokio::time::timeout(
                            Duration::from_secs(lcd_update_interval as u64),
                            moved_lcd_scheduler.update_lcd(),
                        ).await {
                            error!("LCD Scheduler timed out after {} seconds", lcd_update_interval);
                        };
                    });
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
