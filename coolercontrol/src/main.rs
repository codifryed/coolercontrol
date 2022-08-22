/*
 * Coolero - monitor and control your cooling and other devices
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

use std::borrow::Borrow;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use log::{debug, error, info, LevelFilter, warn};
use simple_logger::SimpleLogger;
use sysinfo::{System, SystemExt};
use systemd_journal_logger::connected_to_journal;

use crate::device::Device;
use crate::liqctld_client::Client;
use crate::liquidctl::liqctld_client;
use crate::liquidctl::liquidctl_repo::LiquidctlRepo;
use crate::repository::Repository;

mod liquidctl;
mod device;
mod repository;
mod setting;

/// A program to control your cooling devices
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Enable debug output
    #[clap(long)]
    debug: bool,
}

fn main() -> Result<()> {
    setup_logging();
    let repos = match init_repos() {
        Ok(repos) => repos,
        Err(err) => bail!("Needed Repositories could not be instantiated: {}", err)
    };
    // start_status_updates
    // start_UI_listener

    shutdown(
        &repos
    )
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

fn init_repos() -> Result<Vec<impl Repository>> {
    let liquidctl_repo = match LiquidctlRepo::new() {
        Ok(repo) => repo,
        Err(err) => bail!("Could instantiate the Liquidctl Repository: {}", err)
    };
    liquidctl_repo.initialize_devices();
    Ok(vec![
        liquidctl_repo
    ])
}

fn shutdown(repos: &Vec<impl Repository>) -> Result<()> {
    info!("Shutting down");
    for repo in repos {
        repo.shutdown();
    }
    Ok(())
}