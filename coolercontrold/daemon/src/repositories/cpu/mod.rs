// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod cpu_repo;
mod percent;
mod topology;

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

/// Real cpuinfo dumps, plus the shapes no dump on hand has. Shared, because the topology parses
/// them, the association resolves devices against them, and the repository builds devices from
/// them, so all three assert against the same machines.
#[cfg(test)]
pub mod fixtures {
    macro_rules! cpuinfo {
        ($name:ident, $file:literal) => {
            pub static $name: &str = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/tests/cpuinfo/",
                $file
            ));
        };
    }

    cpuinfo!(AMD_SINGLE_CPU, "amd_single_cpu");
    cpuinfo!(AMD_DOUBLE_CPU, "amd_double_cpu");
    cpuinfo!(INTEL_SINGLE_CPU, "intel_single_cpu");
    cpuinfo!(INTEL_DOUBLE_CPU, "intel_double_cpu");
    cpuinfo!(RASPBERRY_PI_5, "raspberry_pi_5");

    /// Two packages whose physical ids run opposite to their APIC ids. Real firmware rarely does
    /// this, but it is the only shape that tells the two orderings apart.
    pub static INVERTED_APIC: &str = concat!(
        "processor\t: 0\n",
        "model name\t: Test CPU\n",
        "physical id\t: 0\n",
        "apicid\t\t: 32\n",
        "\n",
        "processor\t: 1\n",
        "model name\t: Test CPU\n",
        "physical id\t: 1\n",
        "apicid\t\t: 0\n",
    );

    /// A four package machine with package 1 offline, as cpuinfo shows it: physical ids 0, 2
    /// and 3, which are packages 0, 2 and 3 of the four the kernel numbers zones over.
    pub static PACKAGE_1_OFFLINE: &str = concat!(
        "processor\t: 0\n",
        "model name\t: Test CPU\n",
        "physical id\t: 0\n",
        "apicid\t\t: 0\n",
        "\n",
        "processor\t: 2\n",
        "model name\t: Test CPU\n",
        "physical id\t: 2\n",
        "apicid\t\t: 64\n",
        "\n",
        "processor\t: 3\n",
        "model name\t: Test CPU\n",
        "physical id\t: 3\n",
        "apicid\t\t: 96\n",
    );
}
