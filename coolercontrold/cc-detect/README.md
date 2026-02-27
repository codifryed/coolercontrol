# cc-detect

Super-I/O hardware detection for Linux. Probes I/O ports to identify Super-I/O chips and optionally
loads the appropriate kernel modules via `modprobe`.

This crate is **x86_64 only** - Super-I/O I/O port probing is architecture-specific. On other
architectures `run_detection` is a no-op that returns empty results.

## What is a Super-I/O chip?

Super-I/O chips are hardware monitoring controllers found on most desktop and server motherboards
(ITE, Winbond/Nuvoton, SMSC, National Semiconductor). They expose fan speeds, temperatures, and
voltages through `hwmon` kernel drivers such as `it87`, `nct6775`, and `w83627hf`.

## How it works

Detection uses a two-path algorithm inspired by the
[TsunamiMommy/lm-sensors](https://github.com/TsunamiMommy/lm-sensors) fork:

1. **Fast path (non-invasive)** - reads the chip ID register directly from `/dev/port` without
   entering config mode. Safe and preferred.
2. **Fallback path** - enters config mode using family-specific password sequences (ITE, Winbond,
   SMSC, National Semiconductor) and retries. Always exits config mode after probing, even on error.

Both standard I/O addresses are probed: `0x2E/0x2F` (primary) and `0x4E/0x4F` (secondary).

## Chip database

Chip definitions are compiled into the binary from TOML data files:

- `data/superio_ite.toml`
- `data/superio_winbond.toml`
- `data/superio_smsc.toml`
- `data/superio_national_semi.toml`

An optional runtime override file can be passed to `run_detection` to merge additional chip
definitions without recompiling.

## Usage

```rust
use std::path::Path;
use cc_detect::{run_detection, output_results};

// Detect chips only, no override file, no module loading
let results = run_detection(false, None);
output_results(&results);

// Detect with a custom override file and load kernel modules via modprobe
let override_path = Path::new("/etc/myapp/detect.toml");
let results = run_detection(true, Some(override_path));
for chip in &results.detected_chips {
    println!("{} ({}): {}", chip.name, chip.driver, chip.module_status);
}
```

### `DetectionResults`

```rust
pub struct DetectionResults {
    pub detected_chips: Vec<DetectedChipInfo>, // chips found on the system
    pub skipped: Vec<SkippedDriver>,           // drivers skipped due to conflicts
    pub blacklisted: Vec<String>,              // drivers blacklisted by the kernel
    pub environment: EnvironmentInfo,          // /dev/port availability, container check
}
```

`module_status` on each `DetectedChipInfo` is one of:

| Value                       | Meaning                                        |
| --------------------------- | ---------------------------------------------- |
| `detection_only`            | Module loading was not requested               |
| `loaded`                    | Module loaded successfully                     |
| `already_loaded`            | Module was already present in the kernel       |
| `blacklisted`               | Module is blacklisted (`/etc/modprobe.d/`)     |
| `skipped_conflict_<driver>` | Skipped in favor of a preferred driver         |
| `failed: <reason>`          | `modprobe` failed                              |
| `skipped_no_modprobe`       | `modprobe` not found or running in a container |

## Requirements

- Linux x86_64
- Read access to `/dev/port` (requires `CAP_SYS_RAWIO` or root)
- `modprobe` in `PATH` for module loading (optional)

## Feature flags

None. Dependencies are minimal.

## Contributing

See the project-wide
[CONTRIBUTING.md](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md) for
the general workflow (fork, branch, MR) and code of conduct.

### Adding or correcting chip entries

The most common contribution is adding a missing chip or fixing an incorrect device ID. Each chip
family has its own TOML file in `data/`:

```toml
[[chips]]
name    = "ITE IT8XXX Super IO Sensors"  # human-readable name
driver  = "it87"                         # kernel module (without .ko)
devid   = 0x8xxx                         # 16-bit device ID from the datasheet
devid_mask = 0xFFFF                      # mask applied before comparison (use 0xFF00 for 8-bit IDs)
logdev  = 0x04                           # logical device number for the hwmon function
features = ["voltage", "fan", "temp"]   # any subset of these three
```

After editing a data file, verify the change compiles and all tests pass:

```bash
cargo test
```

### Adding a new chip family

New families with non-standard config-mode entry sequences go in a new TOML file. Add the file to
`src/chip_db.rs` alongside the existing `include_str!` constants and update the
`ChipDatabase::load_compiled` call.

### Tests

Each new detection path should have a corresponding unit test in `src/superio.rs` using
`MockPortIo`. Follow the existing patterns for sequencing `inb` return values.

### Formatting

Run `rustfmt` before submitting. From the repo root:

```bash
make ci-fmt
```

## Attribution

This crate is a Rust reimplementation derived from the `sensors-detect` script in
[lm-sensors](https://github.com/lm-sensors/lm-sensors) and the
[TsunamiMommy/lm-sensors](https://github.com/TsunamiMommy/lm-sensors) fork.

- **lm-sensors** - Copyright (C) the lm-sensors contributors. Licensed under the GNU General Public
  License, version 2 or later (GPL-2.0-or-later).
- **TsunamiMommy/lm-sensors** - fork that introduced the two-path detection algorithm to avoid
  hardware issues on Gigabyte and other motherboards.

The chip databases in `data/` are likewise derived from lm-sensors chip definitions and carry the
same GPL-2.0-or-later terms.

## License

GPL-3.0-or-later (compatible with the GPL-2.0-or-later upstream) - part of the
[CoolerControl](https://gitlab.com/coolercontrol/coolercontrol) project.
