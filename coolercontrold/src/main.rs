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

use std::collections::HashMap;
use std::ops::{Add, Not};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::Parser;
use clokwerk::{AsyncScheduler, Interval};
use env_logger::Logger;
use log::{error, info, trace, warn, LevelFilter, Log, Metadata, Record, SetLoggerError};
use nix::unistd::Uid;
use repositories::custom_sensors_repo::CustomSensorsRepo;
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use systemd_journal_logger::{connected_to_journal, JournalLog};
use tokio::time::sleep;
use tokio::time::Instant;

use repositories::repository::Repository;

use crate::config::Config;
use crate::device::{Device, DeviceType, DeviceUID};
use crate::processing::settings::SettingsController;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu_repo::GpuRepo;
use crate::repositories::hwmon::hwmon_repo::HwmonRepo;
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::repository::{DeviceList, DeviceLock};
use crate::sleep_listener::SleepListener;

mod admin;
mod api;
mod config;
mod device;
mod modes;
mod processing;
mod repositories;
mod setting;
mod sleep_listener;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
const LOG_ENV: &str = "COOLERCONTROL_LOG";

type Repos = Arc<Vec<Arc<dyn Repository>>>;
type AllDevices = Arc<HashMap<DeviceUID, DeviceLock>>;

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

    /// Makes a backup of your current daemon and UI settings
    #[clap(long, short)]
    backup: bool,

    /// Reset the UI password to the default
    #[clap(long)]
    reset_password: bool,
}

/// Main Control Loop
#[tokio::main]
async fn main() -> Result<()> {
    let cmd_args: Args = Args::parse();
    setup_logging(&cmd_args)?;
    info!("Initializing...");
    let term_signal = setup_term_signal()?;
    if !Uid::effective().is_root() {
        return Err(anyhow!("coolercontrold must be run with root permissions"));
    }
    let config = Arc::new(Config::load_config_file().await?);
    if cmd_args.config {
        std::process::exit(0);
    } else if cmd_args.backup {
        info!("Backing up current UI configuration to config-ui-bak.toml");
        match config.load_ui_config_file().await {
            Ok(ui_settings) => config.save_backup_ui_config_file(&ui_settings).await?,
            Err(_) => warn!("Unable to load UI configuration file, skipping."),
        }
        info!("Backing up current daemon configuration to config-bak.toml");
        return config.save_backup_config_file().await;
    } else if cmd_args.reset_password {
        info!("Resetting UI password to default");
        return admin::reset_passwd().await;
    }
    config.save_config_file().await?; // verifies write-ability
    admin::load_passwd().await?;
    // Due to upstream issue https://github.com/mdsherry/clokwerk/issues/38 we need to use UTC:
    let mut scheduler = AsyncScheduler::with_tz(Utc);

    pause_before_startup(&config).await?;

    let mut init_repos: Vec<Arc<dyn Repository>> = vec![];
    match init_liquidctl_repo(config.clone()).await {
        // should be first as it's the slowest
        Ok(repo) => init_repos.push(repo),
        Err(err) => error!("Error initializing LIQUIDCTL Repo: {}", err),
    };
    match init_cpu_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing CPU Repo: {}", err),
    }
    match init_gpu_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing GPU Repo: {}", err),
    }
    match init_hwmon_repo(config.clone()).await {
        Ok(repo) => init_repos.push(Arc::new(repo)),
        Err(err) => error!("Error initializing HWMON Repo: {}", err),
    }
    // should be last as it uses all other device temps
    let devices_for_custom_sensors = collect_all_devices(&init_repos).await;
    let custom_sensors_repo =
        Arc::new(init_custom_sensors_repo(config.clone(), devices_for_custom_sensors).await?);
    init_repos.push(custom_sensors_repo.clone());

    let repos: Repos = Arc::new(init_repos);

    let mut all_devices = HashMap::new();
    for repo in repos.iter() {
        for device_lock in repo.devices().await {
            let uid = device_lock.read().await.uid.clone();
            all_devices.insert(uid, Arc::clone(&device_lock));
        }
    }
    let all_devices: AllDevices = Arc::new(all_devices);
    config.create_device_list(all_devices.clone()).await?;
    config.save_config_file().await?;
    config
        .update_deprecated_settings(all_devices.clone())
        .await?;
    let settings_controller = Arc::new(SettingsController::new(
        all_devices.clone(),
        repos.clone(),
        config.clone(),
    ));

    let mode_controller = Arc::new(
        modes::ModeController::init(
            config.clone(),
            all_devices.clone(),
            settings_controller.clone(),
        )
        .await?,
    );

    mode_controller.handle_settings_at_boot().await;

    let sleep_listener = SleepListener::new()
        .await
        .with_context(|| "Creating DBus Sleep Listener")?;

    match api::init_server(
        all_devices.clone(),
        settings_controller.clone(),
        config.clone(),
        custom_sensors_repo,
        mode_controller.clone(),
    )
    .await
    {
        Ok(server) => {
            tokio::task::spawn(server);
        }
        Err(err) => error!("Error initializing API Server: {}", err),
    };

    add_preload_jobs_into(&mut scheduler, &repos);
    add_status_snapshot_job_into(&mut scheduler, &repos, &settings_controller);
    add_lcd_update_job_into(&mut scheduler, &settings_controller);

    // give concurrent services a moment to come up:
    sleep(Duration::from_millis(10)).await;
    info!("Daemon successfully initialized");
    // main loop:
    while !term_signal.load(Ordering::Relaxed) {
        if sleep_listener.is_waking_up() {
            // delay at least a second to allow the hardware to fully wake up:
            sleep(
                config
                    .get_settings()
                    .await?
                    .startup_delay
                    .max(Duration::from_secs(1)),
            )
            .await;
            if config.get_settings().await?.apply_on_boot {
                info!("Re-initializing and re-applying settings after waking from sleep");
                settings_controller.reinitialize_devices().await;
                mode_controller.apply_all_saved_device_settings().await;
            }
            settings_controller
                .reinitialize_all_status_histories()
                .await;
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

fn setup_logging(cmd_args: &Args) -> Result<()> {
    let version = VERSION.unwrap_or("unknown");
    let log_level = if cmd_args.debug {
        LevelFilter::Debug
    } else if let Ok(log_lvl) = std::env::var(LOG_ENV) {
        LevelFilter::from_str(&log_lvl).unwrap_or(LevelFilter::Info)
    } else {
        LevelFilter::Info
    };
    CCLogger::new(log_level, version)?.init()?;
    info!("Logging Level: {}", log::max_level());
    if log::max_level() == LevelFilter::Debug || cmd_args.version {
        info!(
            "\n\
            CoolerControlD v{}\n\n\
            System:\n\
            \t{}\n\
            \t{}\n\
            ",
            version,
            sysinfo::System::long_os_version().unwrap_or_default(),
            sysinfo::System::kernel_version().unwrap_or_default(),
        );
    }
    if cmd_args.version {
        std::process::exit(0);
    }
    Ok(())
}

fn setup_term_signal() -> Result<Arc<AtomicBool>> {
    let term_signal = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGTERM, Arc::clone(&term_signal))?;
    signal_hook::flag::register(SIGINT, Arc::clone(&term_signal))?;
    signal_hook::flag::register(SIGQUIT, Arc::clone(&term_signal))?;
    Ok(term_signal)
}

/// Some hardware needs additional time to come up and be ready to communicate.
/// Additionally we always add a short pause here to at least allow the liqctld service to come up.
async fn pause_before_startup(config: &Arc<Config>) -> Result<()> {
    sleep(
        config
            .get_settings()
            .await?
            .startup_delay
            .add(Duration::from_secs(1)),
    )
    .await;
    Ok(())
}

/// Liquidctl devices should be first and requires a bit of special handling.
async fn init_liquidctl_repo(config: Arc<Config>) -> Result<Arc<LiquidctlRepo>> {
    let mut lc_repo = LiquidctlRepo::new(config).await?;
    lc_repo.get_devices().await?;
    lc_repo.initialize_devices().await?;
    let lc_repo = Arc::new(lc_repo);
    Arc::clone(&lc_repo).preload_statuses().await;
    lc_repo.update_temp_infos().await;
    lc_repo.update_statuses().await?;
    lc_repo
        .initialize_all_device_status_histories_with_current_status()
        .await;
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

async fn init_custom_sensors_repo(
    config: Arc<Config>,
    devices_for_custom_sensors: DeviceList,
) -> Result<CustomSensorsRepo> {
    let mut custom_sensors_repo = CustomSensorsRepo::new(config, devices_for_custom_sensors).await;
    custom_sensors_repo.initialize_devices().await?;
    Ok(custom_sensors_repo)
}

/// Create separate list of devices to be used in the custom sensors repository
async fn collect_all_devices(init_repos: &[Arc<dyn Repository>]) -> DeviceList {
    let mut devices_for_composite = Vec::new();
    for repo in init_repos {
        if repo.device_type() != DeviceType::CustomSensors {
            for device_lock in repo.devices().await {
                devices_for_composite.push(Arc::clone(&device_lock));
            }
        }
    }
    devices_for_composite
}

/// This Job will run the status preload task for every repository individually.
/// This allows each repository to handle it's own timings for it's devices and be independent
/// of the status snapshots that will happen irregardless every second.
fn add_preload_jobs_into(scheduler: &mut AsyncScheduler<Utc>, repos: &Repos) {
    for repo in repos.iter() {
        let pass_repo = Arc::clone(repo);
        scheduler.every(Interval::Seconds(1)).run(move || {
            let moved_repo = Arc::clone(&pass_repo);
            Box::pin(async move {
                tokio::task::spawn(async move {
                    trace!(
                        "STATUS PRELOAD triggered for {} repo",
                        moved_repo.device_type()
                    );
                    moved_repo.preload_statuses().await;
                });
            })
        });
    }
}

/// This job should snapshot the status of each device in each repository as it is now.
/// This allows us to have a steady stream of status updates consistently every second,
/// regardless of if some device is particularly slow for a while or not.
fn add_status_snapshot_job_into(
    scheduler: &mut AsyncScheduler<Utc>,
    repos: &Repos,
    settings_controller: &Arc<SettingsController>,
) {
    let pass_repos = Arc::clone(repos);
    let pass_settings_controller = Arc::clone(settings_controller);
    scheduler.every(Interval::Seconds(1)).run(move || {
        // we need to pass the references in twice
        let moved_repos = Arc::clone(&pass_repos);
        let moved_settings_controller = Arc::clone(&pass_settings_controller);
        Box::pin(async move {
            // sleep used to attempt to place the jobs appropriately in time after preloading,
            // as they tick off at the same time per second.
            sleep(Duration::from_millis(400)).await;
            trace!("STATUS SNAPSHOTS triggered");
            let start_initialization = Instant::now();
            for repo in moved_repos.iter() {
                // custom sensors should be updated after all real devices
                //  so they should definitely be last in the list
                if let Err(err) = repo.update_statuses().await {
                    error!("Error trying to update status: {}", err);
                }
            }
            trace!(
                "STATUS SNAPSHOT Time taken for all devices: {:?}",
                start_initialization.elapsed()
            );
            moved_settings_controller.update_scheduled_speeds().await;
        })
    });
}

/// The LCD Update job that often takes a long time (>1.0s, <2.0s). It runs in it's own thread to
/// not affect the other jobs in the main loop, but will also timeout to keep very long running
/// jobs from pilling up.
fn add_lcd_update_job_into(
    scheduler: &mut AsyncScheduler<Utc>,
    settings_controller: &Arc<SettingsController>,
) {
    let pass_lcd_processor = Arc::clone(&settings_controller.lcd_commander);
    let lcd_update_interval = 2_u32;
    scheduler
        .every(Interval::Seconds(lcd_update_interval))
        .run(move || {
            // we need to pass the references in twice
            let moved_lcd_processor = Arc::clone(&pass_lcd_processor);
            Box::pin(async move {
                // sleep used to attempt to place the jobs appropriately in time
                // as they tick off at the same time per second.
                sleep(Duration::from_millis(500)).await;
                tokio::task::spawn(async move {
                    if tokio::time::timeout(
                        Duration::from_secs(u64::from(lcd_update_interval)),
                        moved_lcd_processor.update_lcd(),
                    )
                    .await
                    .is_err()
                    {
                        error!(
                            "LCD Scheduler timed out after {} seconds",
                            lcd_update_interval
                        );
                    };
                });
            })
        });
}

async fn shutdown(repos: Repos) -> Result<()> {
    info!("Main process shutting down");
    for repo in repos.iter() {
        if let Err(err) = repo.shutdown().await {
            error!("Shutdown error: {}", err);
        };
    }
    info!("Shutdown Complete");
    Ok(())
}

/// This is our own Logger, which handles appropriate logging dependant on the environment.
struct CCLogger {
    max_level: LevelFilter,
    log_filter: Logger,
    logger: Box<dyn Log>,
}

impl CCLogger {
    fn new(max_level: LevelFilter, version: &str) -> Result<Self> {
        // set library logging levels to one level above the application's to keep chatter down
        let lib_log_level = if max_level == LevelFilter::Trace {
            LevelFilter::Debug
        } else if max_level == LevelFilter::Debug {
            LevelFilter::Info
        } else {
            LevelFilter::Warn
        };
        let timestamp_precision = if max_level == LevelFilter::Debug {
            env_logger::fmt::TimestampPrecision::Millis
        } else {
            env_logger::fmt::TimestampPrecision::Seconds
        };
        let logger: Box<dyn Log> = if connected_to_journal() {
            Box::new(JournalLog::new()?.with_extra_fields(vec![("VERSION", version)]))
        } else {
            Box::new(
                env_logger::Builder::new()
                    .filter_level(max_level)
                    .format_timestamp(Some(timestamp_precision))
                    .build(),
            )
        };
        Ok(Self {
            max_level,
            log_filter: env_logger::Builder::from_env(LOG_ENV)
                .filter_level(max_level)
                .filter_module("zbus", lib_log_level)
                .filter_module("tracing", lib_log_level)
                .filter_module("actix_server", lib_log_level)
                // hyper now uses tracing, but doesn't seem to log as other "tracing crates" do.
                .filter_module("hyper", lib_log_level)
                .build(),
            logger,
        })
    }

    fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(self.max_level);
        log::set_boxed_logger(Box::new(self))
    }
}

impl Log for CCLogger {
    /// Whether this logger is enabled.
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.log_filter.enabled(metadata)
    }

    /// Logs the messages and filters them by matching against the `env_logger` filter
    fn log(&self, record: &Record) {
        if self.log_filter.matches(record) {
            self.logger.log(record);
        }
    }

    /// Flush log records.
    ///
    /// A no-op for this implementation.
    fn flush(&self) {}
}
