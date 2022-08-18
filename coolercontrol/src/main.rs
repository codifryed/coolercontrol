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
    // startup();
    let client = match connect_liqctld() {
        Ok(client) => client,
        Err(err) => {
            error!("Liquidctl Client connection failed: {}", err);
            bail!("{}", err)
        }
    };
    shutdown(&client)?;
    Ok(())
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

fn connect_liqctld() -> Result<Client> {
    let mut retry_count: u8 = 0;
    while retry_count < 5 {
        match Client::new() {
            Ok(client) => {
                match client.handshake() {
                    Ok(()) => return Ok(client),
                    Err(err) => error!("Liqctld handshake error: {}", err)
                };
            }
            Err(err) =>
                error!(
                    "Could not establish liqctld socket connection, retry #{}. \n{}",
                    retry_count, err
                )
        };
        sleep(Duration::from_secs(1));
        retry_count += 1;
    }
    bail!("Failed to connect to liqctld after {} retries", retry_count);
}

fn shutdown(client: &Client) -> Result<()> {
    client.quit()
}