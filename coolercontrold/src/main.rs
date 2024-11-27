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
use std::ops::Add;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;
use crate::device::{Device, DeviceType, DeviceUID};
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu::gpu_repo::GpuRepo;
use crate::repositories::hwmon::hwmon_repo::HwmonRepo;
use crate::repositories::liquidctl::liquidctl_repo::{InitError, LiquidctlRepo};
use crate::repositories::repository::{DeviceList, DeviceLock, Repositories};
use anyhow::{anyhow, Error, Result};
use clap::Parser;
use env_logger::Logger;
use log::{error, info, warn, LevelFilter, Log, Metadata, Record, SetLoggerError};
use nix::sched::{sched_getcpu, sched_setaffinity, CpuSet};
use nix::unistd::{Pid, Uid};
use repositories::custom_sensors_repo::CustomSensorsRepo;
use repositories::repository::Repository;
use systemd_journal_logger::{connected_to_journal, JournalLog};
use tokio::signal;
use tokio::signal::unix::SignalKind;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

mod admin;
mod api;
mod cc_fs;
mod config;
mod device;
mod main_loop;
mod modes;
mod processing;
mod repositories;
mod setting;
mod sleep_listener;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
const LOG_ENV: &str = "COOLERCONTROL_LOG";

type Repos = Rc<Repositories>;
type AllDevices = Rc<HashMap<DeviceUID, DeviceLock>>;

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

    /// Force use of CLI tools instead of NVML for Nvidia GPU monitoring and control
    #[clap(long)]
    nvidia_cli: bool,
}

/// `CoolerControl` uses a single-threaded asynchronous runtime with `io_uring` support.
/// It uses a structured concurrency model combined with `io_uring` for consistent and efficient
/// performance while concurrently handling varying device latencies.
fn main() -> Result<()> {
    let cmd_args: Args = Args::parse();
    setup_logging(&cmd_args)?;
    if !Uid::effective().is_root() {
        return Err(anyhow!("coolercontrold must be run with root permissions"));
    }
    cc_fs::runtime(async {
        let run_token = setup_termination_signals();
        #[cfg(feature = "io_uring")]
        cc_fs::register_uring_buffers()?;
        let config = Rc::new(Config::load_config_file().await?);
        parse_cmd_args(&cmd_args, &config).await?;
        config.verify_writeability().await?;
        admin::load_passwd().await?;

        pause_before_startup(&config).await?;
        let (repos, custom_sensors_repo) = initialize_device_repos(&config, &cmd_args).await?;
        let all_devices = create_devices_map(&repos).await;
        config.create_device_list(all_devices.clone()).await?;
        let settings_controller = Rc::new(SettingsController::new(
            all_devices.clone(),
            repos.clone(),
            config.clone(),
        ));
        let mode_controller = Rc::new(
            ModeController::init(
                config.clone(),
                all_devices.clone(),
                settings_controller.clone(),
            )
            .await?,
        );

        moro_local::async_scope!(|main_scope| -> Result<()> {
            mode_controller.handle_settings_at_boot().await;
            if let Err(err) = api::start_server(
                all_devices,
                settings_controller.clone(),
                config.clone(),
                custom_sensors_repo,
                mode_controller.clone(),
                run_token.clone(),
                main_scope,
            )
            .await
            {
                error!("Error initializing API Server: {err}");
            };

            // give concurrent services a moment to finish initializing:
            sleep(Duration::from_millis(10)).await;
            set_cpu_affinity()?;
            info!("Initialization Complete");
            main_loop::run(
                Rc::clone(&config),
                Rc::clone(&repos),
                settings_controller,
                mode_controller,
                run_token,
            )
            .await?;
            Ok(())
        })
        .await?;
        // all tasks from the main scope must have completed by this point.
        shutdown(repos, config).await
    })
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
            CoolerControlD v{version}\n\n\
            System:\n\
            \t{}\n\
            \t{}\n\
            ",
            sysinfo::System::long_os_version().unwrap_or_default(),
            sysinfo::System::kernel_version().unwrap_or_default(),
        );
    } else {
        info!(
            "Initializing CoolerControl {version} running on Kernel {}",
            sysinfo::System::kernel_version().unwrap_or_default()
        );
    }
    if cmd_args.version {
        exit_successfully();
    }
    Ok(())
}

/// Sets up signal handlers for termination and interrupt signals,
/// and returns a `CancellationToken` that is triggered when any of
/// those signals are received, allowing the caller to handle the
/// signal gracefully.
///
/// # Errors
///
/// This function returns an error if there is a problem setting up
/// the signal handlers.
fn setup_termination_signals() -> CancellationToken {
    let run_token = CancellationToken::new();
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    let sigterm = async {
        signal::unix::signal(SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    let sigint = async {
        signal::unix::signal(SignalKind::interrupt())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    let sigquit = async {
        signal::unix::signal(SignalKind::quit())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    let sig_run_token = run_token.clone();
    tokio::task::spawn_local(async move {
        tokio::select! {
            () = ctrl_c => {},
            () = sigterm => {},
            () = sigint => {},
            () = sigquit => {},
        }
        sig_run_token.cancel();
    });
    run_token
}

async fn parse_cmd_args(cmd_args: &Args, config: &Rc<Config>) -> Result<()> {
    if cmd_args.config {
        exit_successfully();
    } else if cmd_args.backup {
        info!("Backing up current UI configuration to config-ui-bak.toml");
        if let Ok(ui_settings) = config.load_ui_config_file().await {
            config.save_backup_ui_config_file(ui_settings).await?;
        } else {
            warn!("Unable to load UI configuration file, skipping.");
        }
        info!("Backing up current daemon configuration to config-bak.toml");
        match config.save_backup_config_file().await {
            Ok(()) => exit_successfully(),
            Err(err) => exit_with_error(&err),
        };
    } else if cmd_args.reset_password {
        info!("Resetting UI password to default");
        match admin::reset_passwd().await {
            Ok(()) => exit_successfully(),
            Err(err) => exit_with_error(&err),
        }
    }
    Ok(())
}

fn exit_with_error(err: &Error) {
    error!("{err}");
    std::process::exit(1);
}

fn exit_successfully() {
    std::process::exit(0)
}

/// Some hardware needs additional time to come up and be ready to communicate.
/// Additionally, we always add a short pause here to at least allow the `liqctld` service to come up.
async fn pause_before_startup(config: &Rc<Config>) -> Result<()> {
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

async fn initialize_device_repos(
    config: &Rc<Config>,
    cmd_args: &Args,
) -> Result<(Repos, Rc<CustomSensorsRepo>)> {
    info!("Initializing Devices...");
    let mut repos = Repositories::default();
    let mut lc_locations = Vec::new();
    // liquidctl should be first
    match init_liquidctl_repo(config.clone()).await {
        Ok((repo, mut lc_locs)) => {
            lc_locations.append(&mut lc_locs);
            repos.liquidctl = Some(repo);
        }
        Err(err) if err.downcast_ref() == Some(&InitError::Disabled) => info!("{err}"),
        Err(err) => warn!("Error initializing LIQUIDCTL Repo: {err}"),
    };
    // init these concurrently:
    moro_local::async_scope!(|init_scope| {
        init_scope.spawn(async {
            match init_cpu_repo(config.clone()).await {
                Ok(repo) => repos.cpu = Some(Rc::new(repo)),
                Err(err) => error!("Error initializing CPU Repo: {err}"),
            }
        });
        init_scope.spawn(async {
            match init_gpu_repo(config.clone(), cmd_args.nvidia_cli).await {
                Ok(repo) => repos.gpu = Some(Rc::new(repo)),
                Err(err) => error!("Error initializing GPU Repo: {err}"),
            }
        });
        init_scope.spawn(async {
            match init_hwmon_repo(config.clone(), lc_locations).await {
                Ok(repo) => repos.hwmon = Some(Rc::new(repo)),
                Err(err) => error!("Error initializing HWMON Repo: {err}"),
            }
        });
    })
    .await;
    // should be last as it uses all other device temps
    let devices_for_custom_sensors = collect_all_devices(&repos).await;
    let custom_sensors_repo =
        Rc::new(init_custom_sensors_repo(config.clone(), devices_for_custom_sensors).await?);
    repos.custom_sensors = Some(custom_sensors_repo.clone());
    Ok((Rc::new(repos), custom_sensors_repo))
}

/// Liquidctl devices should be first and requires a bit of special handling.
async fn init_liquidctl_repo(config: Rc<Config>) -> Result<(Rc<LiquidctlRepo>, Vec<String>)> {
    let mut lc_repo = LiquidctlRepo::new(config).await?;
    lc_repo.get_devices().await?;
    lc_repo.initialize_devices().await?;
    let lc_locations = lc_repo.get_all_driver_locations().await;
    let lc_repo = Rc::new(lc_repo);
    Rc::clone(&lc_repo).preload_statuses().await;
    lc_repo.update_temp_infos().await;
    lc_repo.update_statuses().await?;
    lc_repo
        .initialize_all_device_status_histories_with_current_status()
        .await;
    Ok((lc_repo, lc_locations))
}

async fn init_cpu_repo(config: Rc<Config>) -> Result<CpuRepo> {
    let mut cpu_repo = CpuRepo::new(config)?;
    cpu_repo.initialize_devices().await?;
    Ok(cpu_repo)
}

async fn init_gpu_repo(config: Rc<Config>, nvidia_cli: bool) -> Result<GpuRepo> {
    let mut gpu_repo = GpuRepo::new(config, nvidia_cli).await?;
    gpu_repo.initialize_devices().await?;
    Ok(gpu_repo)
}

async fn init_hwmon_repo(config: Rc<Config>, lc_locations: Vec<String>) -> Result<HwmonRepo> {
    let mut hwmon_repo = HwmonRepo::new(config, lc_locations);
    hwmon_repo.initialize_devices().await?;
    Ok(hwmon_repo)
}

async fn init_custom_sensors_repo(
    config: Rc<Config>,
    devices_for_custom_sensors: DeviceList,
) -> Result<CustomSensorsRepo> {
    let mut custom_sensors_repo = CustomSensorsRepo::new(config, devices_for_custom_sensors).await;
    custom_sensors_repo.initialize_devices().await?;
    Ok(custom_sensors_repo)
}

/// Create separate list of devices to be used in the custom sensors repository
async fn collect_all_devices(repos: &Repositories) -> DeviceList {
    let mut devices_for_composite = Vec::new();
    for repo in repos.iter() {
        if repo.device_type() != DeviceType::CustomSensors {
            for device_lock in repo.devices().await {
                devices_for_composite.push(Rc::clone(&device_lock));
            }
        }
    }
    devices_for_composite
}

async fn create_devices_map(repos: &Repos) -> AllDevices {
    let mut all_devices = HashMap::new();
    for repo in repos.iter() {
        for device_lock in repo.devices().await {
            let uid = device_lock.borrow().uid.clone();
            all_devices.insert(uid, Rc::clone(&device_lock));
        }
    }
    Rc::new(all_devices)
}

/// This will make sure that our main tokio task thread stays on the same CPU, reducing
/// any unnecessary context switching.
///
/// The downside is that the blocking IO thread pool is generally a bit larger, but still
/// less than the standard multithreaded setup of one thread per core. Due to this, it should
/// not be called until the main initialization work has been completed.
fn set_cpu_affinity() -> Result<()> {
    let current_cpu = sched_getcpu()?;
    let mut cpu_set = CpuSet::new();
    cpu_set.set(current_cpu)?;
    sched_setaffinity(Pid::from_raw(0), &cpu_set)?;
    Ok(())
}

async fn shutdown(repos: Repos, config: Rc<Config>) -> Result<()> {
    info!("Main process shutting down");
    // verifies all config document locks have been released before shutdown:
    config.save_config_file().await.unwrap_or_default();
    for repo in repos.iter() {
        if let Err(err) = repo.shutdown().await {
            error!("Shutdown error: {}", err);
        };
    }
    info!("Shutdown Complete");
    Ok(())
}

/// This is our own Logger, which handles appropriate logging dependent on the environment.
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
                .filter_module("aide", lib_log_level)
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
