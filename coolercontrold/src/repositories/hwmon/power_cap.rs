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

use crate::cc_fs;
use crate::device::Watts;
use crate::repositories::cpu_repo::CPU_POWER_NAME;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};
use anyhow::{Context, Result};
use log::{debug, info, trace};
use nu_glob::glob;
use regex::Regex;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const GLOB_RAPL_ENERGY_PATH: &str = "/sys/class/powercap/intel-rapl:?/energy_uj";
const PATTERN_RAPL_ZONE_NUMBER: &str = r"/(?P<rapl>intel-rapl):(?P<number>\d+)";
const PATTERN_RAPL_PACKAGE_NUMBER: &str = r"^package-(?P<number>\d+)$";

/// Find the power cap devices that have usable energy monitors.
///
/// Notes:
/// * We use the main `PowerCap`/`RAPL` Zone, which contains the usage for all sub zones as well.
///   i.e. `intel-rapl:0` contains the usage for `intel-rapl:0:0`, `intel-rapl:0:1`, etc.
/// * AMD has a slightly different implementation that Intel does, so one can not do
///   a 1:1 comparison.
/// * Measurements contain only CPU power, and sometimes DRAM power usage.
/// * All measurements are estimates, and are not guaranteed to be actual.
///
/// Sources:
/// [Kernel Docs](https://www.kernel.org/doc/html/next/power/powercap/powercap.html)
/// [Powercap C bindings and utils](https://github.com/powercap/powercap)
/// [Kubernetes Power Monitoring](https://sustainable-computing.io/design/kepler-energy-sources/#rapl-running-average-power-limit)
/// [RAPL Support Info](https://web.eece.maine.edu/~vweaver/projects/rapl/)
pub async fn find_power_cap_paths() -> Result<Vec<HwmonChannelInfo>> {
    let mut power_cap_devices = Vec::new();
    let base_paths = glob(GLOB_RAPL_ENERGY_PATH, None)?
        .filter_map(Result::ok)
        .filter(|path| path.is_absolute())
        .map(|path| path.parent().unwrap().to_path_buf())
        .collect::<Vec<PathBuf>>();
    let regex_package_number = Regex::new(PATTERN_RAPL_PACKAGE_NUMBER)?;
    for path in base_paths {
        let rapl_name = get_rapl_name(&path).await;
        let Some(captures) = regex_package_number.captures(&rapl_name) else {
            debug!("PowerCap driver Name at location: {path:?} is not a package, skipping");
            continue;
        };
        let package_number: u8 = captures
            .name("number")
            .context("Number Group should exist")?
            .as_str()
            .parse()?;
        if energy_is_not_usable(&path).await {
            continue;
        }
        power_cap_devices.push(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::PowerCap,
            number: package_number,
            name: format!("power{package_number}"),
            label: Some(CPU_POWER_NAME.to_string()),
            ..Default::default()
        });
    }
    trace!("PowerCap channels detected: {power_cap_devices:?}");
    Ok(power_cap_devices)
}

/// Extract the power cap energy count in Joules.
pub async fn extract_power_joule_counter(channel_number: u8) -> f64 {
    cc_fs::read_sysfs(format!(
        "/sys/class/powercap/intel-rapl:{channel_number}/energy_uj"
    ))
    .await
    .and_then(check_parsing_f64)
    .map(microjoules_to_joules)
    .unwrap_or_default()
}

/// Calculate the power consumption in Watts from the current and previous energy counters.
/// If this is the first run or the counter has reset, this will return 0.
pub fn calculate_power_watts(joule_count: f64, previous_joule_count: f64, poll_rate: f64) -> Watts {
    (joule_count - previous_joule_count).max(0.) / poll_rate
}

/// Get the name of the `RAPL` package.
/// This is often `package-0`. Note that for multi-physical-cpu systems this is a better
/// indication of which CPU this belongs to than the `RAPL` zone number.
async fn get_rapl_name(base_path: &PathBuf) -> String {
    if let Ok(contents) = cc_fs::read_sysfs(base_path.join("name")).await {
        contents.trim().to_string()
    } else {
        let captures = Regex::new(PATTERN_RAPL_ZONE_NUMBER)
            .unwrap()
            .captures(base_path.to_str().unwrap())
            .unwrap();
        let zone_number = captures.name("number").unwrap().as_str().to_string();
        // Real zone numbers don't always match the package number, but this is only a fallback:
        let rapl_name = format!("package-{zone_number}");
        info!(
            "PowerCap driver at location: {base_path:?} has no name set, using default: {rapl_name}"
        );
        rapl_name
    }
}

/// Check if the energy channel is usable.
async fn energy_is_not_usable(base_path: &PathBuf) -> bool {
    cc_fs::read_sysfs(base_path.join("energy_uj"))
        .await
        .and_then(check_parsing_f64)
        .map(microjoules_to_joules)
        .inspect_err(|err| {
            info!("Error reading energy value from: {base_path:?}/energy_uj - {err}");
        })
        .is_err()
}

fn microjoules_to_joules(microjoules: f64) -> f64 {
    microjoules / 1_000_000.
}

fn check_parsing_f64(content: String) -> Result<f64> {
    match content.trim().parse::<f64>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}
