/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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
use std::rc::Rc;
use std::time::Duration;

use crate::alerts::AlertController;
use crate::config::Config;
use crate::device::{Device, DeviceType, DeviceUID};
use crate::engine::main::Engine;
use crate::modes::ModeController;
use crate::repositories::cpu_repo::CpuRepo;
use crate::repositories::gpu::gpu_repo::GpuRepo;
use crate::repositories::hwmon::hwmon_repo::HwmonRepo;
use crate::repositories::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repositories::repository::{DeviceList, DeviceLock, InitError, Repositories};
use anyhow::{anyhow, Error, Result};
use clap::Parser;
use log::{error, info, warn};
use nix::sched::{sched_getcpu, sched_setaffinity, CpuSet};
use nix::unistd::{Pid, Uid};
use repositories::custom_sensors_repo::CustomSensorsRepo;
use repositories::repository::Repository;
use tokio::signal;
use tokio::signal::unix::SignalKind;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

mod admin;
mod alerts;
mod api;
mod cc_fs;
mod config;
mod device;
mod engine;
mod logger;
mod main_loop;
mod modes;
mod repositories;
mod setting;
mod sleep_listener;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

///
/// Environment Variable: log level
///
/// # Example
/// ```
/// COOLERCONTROL_LOG=DEBUG coolercontrold
/// ```
const ENV_LOG: &str = "COOLERCONTROL_LOG";

/// Environment Variable: log level (short form)
/// Takes a valid upper-case log level
///
/// # Example
/// ```
/// CC_LOG=DEBUG coolercontrold
/// ```
const ENV_CC_LOG: &str = "CC_LOG";

/// Environment Variable: API Port to listen on
/// Takes a valid port string
///
/// # Example
/// ```
/// CC_PORT=11987 coolercontrold
/// ```
const ENV_PORT: &str = "CC_PORT";

/// Environment Variable: API IPv4 to listen on
/// Takes a valid IPv4 String
///
/// # Example
/// ```
/// CC_HOST_IP4=127.0.0.1 coolercontrold
/// ```
const ENV_HOST_IP4: &str = "CC_HOST_IP4";

/// Environment Variable: API IPv6 to listen on
/// Takes a valid IPv6 String
///
/// # Example
/// ```
/// CC_HOST_IP6=::1 coolercontrold
/// ```
const ENV_HOST_IP6: &str = "CC_HOST_IP6";

/// Environment Variable: To disable dbus integration (sleep listener)
/// Takes one of: [`1`, `0`, `ON`, `on`, `OFF`, `off`]
///
/// # Example
/// ```
/// CC_DBUS=ON coolercontrold
/// ```
const ENV_DBUS: &str = "CC_DBUS";

/// Environment Variable: To disable NVML integration
/// Takes one of: [`1`, `0`, `ON`, `on`, `OFF`, `off`]
///
/// # Example
/// ```
/// CC_NVML=ON coolercontrold
/// ```
const ENV_NVML: &str = "CC_NVML";

type Repos = Rc<Repositories>;
type AllDevices = Rc<HashMap<DeviceUID, DeviceLock>>;

/// A program to control your cooling devices
#[allow(clippy::struct_excessive_bools)]
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

/// `coolercontrold` uses a single-threaded asynchronous runtime with optional `io_uring` support.
/// It uses a structured concurrency model for consistent and efficient performance while
/// concurrently handling varying device latencies.
fn main() -> Result<()> {
    let cmd_args: Args = Args::parse();
    if cmd_args.version {
        println!("coolercontrold {VERSION}\n");
    }
    cc_fs::runtime(async {
        let run_token = setup_termination_signals();
        let log_buf_handle = logger::setup_logging(&cmd_args, run_token.clone()).await?;
        verify_is_root()?;
        #[cfg(feature = "io_uring")]
        cc_fs::register_uring_buffers()?;
        let config = Rc::new(Config::load_config_file().await?);
        parse_cmd_args(&cmd_args, &config).await?;
        config.verify_writeability()?;
        admin::load_passwd().await?;

        pause_before_startup(&config).await?;
        let (repos, custom_sensors_repo) =
            initialize_device_repos(&config, &cmd_args, run_token.clone()).await?;
        let all_devices = create_devices_map(&repos).await;
        config.create_device_list(&all_devices);
        let engine = Rc::new(Engine::new(all_devices.clone(), &repos, config.clone()));
        let mode_controller = Rc::new(
            ModeController::init(config.clone(), all_devices.clone(), engine.clone()).await?,
        );

        moro_local::async_scope!(|main_scope| -> Result<()> {
            mode_controller.handle_settings_at_boot().await;
            let status_handle =
                api::actor::StatusHandle::new(all_devices.clone(), run_token.clone(), main_scope);
            let alert_controller = Rc::new(AlertController::init(all_devices.clone()).await?);
            AlertController::watch_for_shutdown(&alert_controller, run_token.clone(), main_scope);
            if let Err(err) = api::start_server(
                all_devices,
                Rc::clone(&repos),
                engine.clone(),
                config.clone(),
                custom_sensors_repo,
                mode_controller.clone(),
                alert_controller.clone(),
                log_buf_handle,
                status_handle.clone(),
                run_token.clone(),
                main_scope,
            )
            .await
            {
                error!("Error initializing API Server: {err}");
            }

            // give concurrent services a moment to finish initializing:
            sleep(Duration::from_millis(10)).await;
            set_cpu_affinity()?;
            info!("Initialization Complete");
            main_loop::run(
                Rc::clone(&config),
                Rc::clone(&repos),
                engine,
                mode_controller,
                alert_controller,
                status_handle,
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

fn verify_is_root() -> Result<()> {
    if Uid::effective().is_root() {
        Ok(())
    } else {
        Err(anyhow!("coolercontrold must be run with root permissions"))
    }
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
async fn pause_before_startup(config: &Rc<Config>) -> Result<()> {
    let startup_delay = config.get_settings()?.startup_delay;
    if startup_delay > Duration::from_secs(2) {
        info!(
            "Waiting {}s before communicating with devices.",
            startup_delay.as_secs()
        );
    }
    sleep(startup_delay).await;
    Ok(())
}

async fn initialize_device_repos(
    config: &Rc<Config>,
    cmd_args: &Args,
    run_token: CancellationToken,
) -> Result<(Repos, Rc<CustomSensorsRepo>)> {
    info!("Initializing Devices...");
    let mut repos = Repositories::default();
    let mut lc_locations = Vec::new();
    // liquidctl should be first
    match init_liquidctl_repo(config.clone(), run_token).await {
        Ok((repo, mut lc_locs)) => {
            lc_locations.append(&mut lc_locs);
            repos.liquidctl = Some(repo);
        }
        Err(err) => match err.downcast_ref() {
            Some(&InitError::LiqctldDisabled) => info!("{err}"),
            Some(&InitError::PythonEnv { .. }) => warn!("{err}"),
            _ => warn!("Error initializing LIQUIDCTL Repo: {err}"),
        },
    }
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
async fn init_liquidctl_repo(
    config: Rc<Config>,
    run_token: CancellationToken,
) -> Result<(Rc<LiquidctlRepo>, Vec<String>)> {
    let mut lc_repo = LiquidctlRepo::new(config, run_token).await?;
    lc_repo.get_devices().await?;
    lc_repo.initialize_devices().await?;
    let lc_locations = lc_repo.get_all_driver_locations();
    let lc_repo = Rc::new(lc_repo);
    Rc::clone(&lc_repo).preload_statuses().await;
    lc_repo.update_temp_infos();
    lc_repo.update_statuses().await?;
    lc_repo.initialize_all_device_status_histories_with_current_status()?;
    Ok((lc_repo, lc_locations))
}

async fn init_cpu_repo(config: Rc<Config>) -> Result<CpuRepo> {
    let mut cpu_repo = CpuRepo::new(config)?;
    cpu_repo.initialize_devices().await?;
    Ok(cpu_repo)
}

async fn init_gpu_repo(config: Rc<Config>, nvidia_cli: bool) -> Result<GpuRepo> {
    let mut gpu_repo = GpuRepo::new(config, nvidia_cli);
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
    let mut custom_sensors_repo = CustomSensorsRepo::new(config, devices_for_custom_sensors);
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
            error!("Shutdown error: {err}");
        };
    }
    info!("Shutdown Complete");
    Ok(())
}
