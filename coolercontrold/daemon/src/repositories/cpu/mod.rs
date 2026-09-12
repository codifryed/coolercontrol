// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod cpu_repo;
mod percent;

/// The prefix every CPU temp label carries, so the sensor reads as a CPU one wherever it is shown.
const CPU_TEMP_NAME: &str = "CPU Temp";
/// The name of the package power channel. The hwmon power cap module builds the channel, so it
/// needs the name the CPU repository presents it under.
pub const CPU_POWER_NAME: &str = "CPU Power";
/// The standard Intel temperature module, which is the one driver that reports per package zone.
const INTEL_DEVICE_NAME: &str = "coretemp";
/// The CPU temperature drivers, in the order they are consulted. The first match wins, so a
/// vendor's standard module is tried before its out-of-tree alternative.
pub const CPU_DEVICE_NAMES_ORDERED: [&str; 4] = [
    "k10temp",         // standard AMD module
    INTEL_DEVICE_NAME, // standard Intel module
    "zenpower",        // zenpower AMD module
    "cpu_thermal",     // Raspberry Pi module
];
