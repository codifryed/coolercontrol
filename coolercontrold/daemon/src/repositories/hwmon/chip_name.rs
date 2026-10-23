// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Derives the libsensors chip name, `prefix-bustype[-busnr]-address`, for an hwmon directory.
//!
//! This is the identity `sensors` prints and the one `sensors.conf` chip statements match against.
//! Mirrors `classify_device` and `find_bus_type` in lm-sensors `lib/sysfs.c`, plus
//! `sensors_snprintf_chip_name` in `lib/data.c`. Reimplemented rather than linked: libsensors
//! exports no way to read the ignore set, so it cannot serve the rest of this feature anyway.
//!
//! Pure function of a sysfs tree, so it is testable against fixture directories.

use crate::cc_fs;
use crate::repositories::hwmon::identity::{IdentityReader, Stalled};
use std::fmt;
use std::ops::Not;
use std::path::{Path, PathBuf};

/// The real sysfs mount. Only the i2c adapter lookup needs it; tests pass a fixture root.
const SYSFS_ROOT: &str = "/sys";

/// libsensors' marker for an ISA adapter that the kernel exposes as an i2c bus.
const I2C_ISA_BUS_NR: i16 = 9191;

/// The name an ISA adapter reports, matched on the prefix as libsensors does.
const I2C_ISA_ADAPTER_PREFIX: &str = "ISA ";

/// Bound on the `device` chain walk taken when a subsystem is unrecognized. Real chains are two or
/// three links, so this only exists to keep a pathological tree from spinning.
const MAX_PARENT_WALK: usize = 16;

/// A chip identity as libsensors builds it. `Display` renders the `sensors` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipName {
    /// The `name` attribute of the hwmon directory, e.g. `nct6687`.
    pub prefix: String,
    pub bus: Bus,
}

/// The bus types libsensors knows, as named in a `chip` statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    Isa,
    Pci,
    I2c,
    Spi,
    Virtual,
    Acpi,
    Hid,
    Mdio,
    Scsi,
    Sdio,
}

impl BusType {
    /// Parses the bus element of a chip pattern, as `sensors_parse_chip_name` does.
    pub fn from_config_token(token: &str) -> Option<Self> {
        match token {
            "isa" => Some(Self::Isa),
            "pci" => Some(Self::Pci),
            "i2c" => Some(Self::I2c),
            "spi" => Some(Self::Spi),
            "virtual" => Some(Self::Virtual),
            "acpi" => Some(Self::Acpi),
            "hid" => Some(Self::Hid),
            "mdio" => Some(Self::Mdio),
            "scsi" => Some(Self::Scsi),
            "sdio" => Some(Self::Sdio),
            _ => None,
        }
    }

    /// Whether this bus type carries a bus number element in a chip pattern.
    pub fn has_bus_nr(self) -> bool {
        matches!(self, Self::I2c | Self::Spi | Self::Hid | Self::Scsi)
    }
}

/// Bus type and address, carrying only the fields that bus type prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Isa { addr: u32 },
    Pci { addr: u32 },
    I2c { nr: i16, addr: u32 },
    Spi { nr: i16, addr: u32 },
    Virtual,
    Acpi,
    Hid { nr: i16, addr: u32 },
    Mdio { addr: u32 },
    Scsi { nr: i16, addr: u32 },
    Sdio { nr: i16, addr: u32 },
}

impl Bus {
    pub fn bus_type(self) -> BusType {
        match self {
            Self::Isa { .. } => BusType::Isa,
            Self::Pci { .. } => BusType::Pci,
            Self::I2c { .. } => BusType::I2c,
            Self::Spi { .. } => BusType::Spi,
            Self::Virtual => BusType::Virtual,
            Self::Acpi => BusType::Acpi,
            Self::Hid { .. } => BusType::Hid,
            Self::Mdio { .. } => BusType::Mdio,
            Self::Scsi { .. } => BusType::Scsi,
            Self::Sdio { .. } => BusType::Sdio,
        }
    }

    /// Zero for the bus types that carry no bus number, matching what libsensors stores.
    pub fn nr(self) -> i16 {
        match self {
            Self::I2c { nr, .. }
            | Self::Spi { nr, .. }
            | Self::Hid { nr, .. }
            | Self::Scsi { nr, .. }
            | Self::Sdio { nr, .. } => nr,
            _ => 0,
        }
    }

    /// Zero for virtual and acpi devices, which libsensors assumes are unique.
    pub fn addr(self) -> u32 {
        match self {
            Self::Isa { addr }
            | Self::Pci { addr }
            | Self::I2c { addr, .. }
            | Self::Spi { addr, .. }
            | Self::Hid { addr, .. }
            | Self::Mdio { addr }
            | Self::Scsi { addr, .. }
            | Self::Sdio { addr, .. } => addr,
            Self::Virtual | Self::Acpi => 0,
        }
    }
}

impl fmt::Display for ChipName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = &self.prefix;
        match self.bus {
            Bus::Isa { addr } => write!(f, "{prefix}-isa-{addr:04x}"),
            Bus::Pci { addr } => write!(f, "{prefix}-pci-{addr:04x}"),
            Bus::I2c { nr, addr } => write!(f, "{prefix}-i2c-{nr}-{addr:02x}"),
            Bus::Spi { nr, addr } => write!(f, "{prefix}-spi-{nr}-{addr:x}"),
            // libsensors assumes virtual and acpi devices are unique, so the address is always 0.
            Bus::Virtual => write!(f, "{prefix}-virtual-0"),
            Bus::Acpi => write!(f, "{prefix}-acpi-0"),
            Bus::Hid { nr, addr } => write!(f, "{prefix}-hid-{nr}-{addr:x}"),
            Bus::Mdio { addr } => write!(f, "{prefix}-mdio-{addr:x}"),
            Bus::Scsi { nr, addr } => write!(f, "{prefix}-scsi-{nr}-{addr:x}"),
            Bus::Sdio { nr, addr } => {
                let slot = addr >> 3;
                let function = addr & 0x7;
                write!(f, "{prefix}-sdio-{nr:x}:{slot:04x}:{function:x}")
            }
        }
    }
}

/// Derives the chip name for an hwmon directory, or `None` if it has no `name` attribute.
///
/// # Errors
///
/// `Stalled` when one of its two attribute reads got no answer.
pub async fn derive(
    hwmon_path: &Path,
    reader: &IdentityReader,
) -> Result<Option<ChipName>, Stalled> {
    derive_in(Path::new(SYSFS_ROOT), hwmon_path, reader).await
}

/// `sysfs_root` is split out only for the i2c adapter lookup, which is an absolute path in
/// libsensors. Everything else is reached relative to `hwmon_path`.
async fn derive_in(
    sysfs_root: &Path,
    hwmon_path: &Path,
    reader: &IdentityReader,
) -> Result<Option<ChipName>, Stalled> {
    let Some(name) = reader.text(&hwmon_path.join("name")).await? else {
        return Ok(None);
    };
    let prefix = name.trim().to_owned();
    if prefix.is_empty() {
        return Ok(None);
    }
    // No device directory, or a chain we cannot classify, is a virtual device.
    let bus = match device_dir(hwmon_path) {
        Some(device_dir) => find_bus(sysfs_root, device_dir, reader)
            .await?
            .unwrap_or(Bus::Virtual),
        None => Bus::Virtual,
    };
    Ok(Some(ChipName { prefix, bus }))
}

/// The device directory the hwmon directory belongs to.
///
/// `CentOS` style paths already end in `device`, in which case they are that directory. Otherwise
/// the `device` entry must be a symlink, matching libsensors' `realpath_no_symlink`.
fn device_dir(hwmon_path: &Path) -> Option<PathBuf> {
    if hwmon_path.ends_with("device") {
        cc_fs::canonicalize(hwmon_path).ok()
    } else {
        resolve_symlink(&hwmon_path.join("device"))
    }
}

/// Walks up the `device` chain until a level classifies, as `find_bus_type` does.
async fn find_bus(
    sysfs_root: &Path,
    device_dir: PathBuf,
    reader: &IdentityReader,
) -> Result<Option<Bus>, Stalled> {
    let mut current = device_dir;
    for _ in 0..MAX_PARENT_WALK {
        let Some(device_name) = current.file_name().and_then(|name| name.to_str()) else {
            return Ok(None);
        };
        let device_name = device_name.to_owned();
        debug_assert!(device_name.is_empty().not());
        let subsystem = read_subsystem(&current);
        if let Some(bus) = classify(sysfs_root, &device_name, subsystem.as_deref(), reader).await? {
            return Ok(Some(bus));
        }
        let Some(parent) = resolve_symlink(&current.join("device")) else {
            return Ok(None);
        };
        current = parent;
    }
    Ok(None)
}

/// The subsystem a device belongs to, from the last element of the `subsystem` link target.
fn read_subsystem(device_dir: &Path) -> Option<String> {
    // `bus` is the pre-2.6.18 spelling of the same link.
    let target = cc_fs::read_link(device_dir.join("subsystem"))
        .or_else(|_| cc_fs::read_link(device_dir.join("bus")))
        .ok()?;
    Some(target.file_name()?.to_str()?.to_owned())
}

/// Resolves a path only if it really is a symlink, as libsensors' `realpath_no_symlink` does.
fn resolve_symlink(path: &Path) -> Option<PathBuf> {
    cc_fs::read_link(path).ok()?;
    cc_fs::canonicalize(path).ok()
}

/// Classifies a device by its subsystem and name, or `None` to keep walking up the chain.
///
/// The if-chain mirrors `classify_device`: the first four branches also accept a missing subsystem,
/// which is why a device with no subsystem link always lands on ISA rather than walking up.
async fn classify(
    sysfs_root: &Path,
    device_name: &str,
    subsystem: Option<&str>,
    reader: &IdentityReader,
) -> Result<Option<Bus>, Stalled> {
    let unknown_subsystem = subsystem.is_none();
    if unknown_subsystem || subsystem == Some("i2c") {
        if let Some((nr, addr)) = parse_i2c(device_name) {
            return Ok(Some(classify_i2c(sysfs_root, nr, addr, reader).await?));
        }
    }
    Ok(classify_without_reads(device_name, subsystem))
}

/// Every branch of `classify` after i2c, none of which reads an attribute.
fn classify_without_reads(device_name: &str, subsystem: Option<&str>) -> Option<Bus> {
    let unknown_subsystem = subsystem.is_none();
    if unknown_subsystem || subsystem == Some("spi") {
        if let Some((nr, addr)) = parse_spi(device_name) {
            return Some(Bus::Spi { nr, addr });
        }
    }
    if unknown_subsystem || subsystem == Some("pci") {
        if let Some(addr) = parse_quad_hex(device_name).and_then(pack_pci_addr) {
            return Some(Bus::Pci { addr });
        }
    }
    if unknown_subsystem || subsystem == Some("platform") || subsystem == Some("of_platform") {
        return Some(Bus::Isa {
            addr: parse_isa_addr(device_name),
        });
    }
    classify_by_subsystem(device_name, subsystem?)
}

fn classify_by_subsystem(device_name: &str, subsystem: &str) -> Option<Bus> {
    match subsystem {
        "acpi" => Some(Bus::Acpi),
        "hid" => {
            let (bus, _vendor, _product, id) = parse_quad_hex(device_name)?;
            Some(Bus::Hid {
                nr: i16::try_from(bus).ok()?,
                addr: id,
            })
        }
        "mdio_bus" => Some(Bus::Mdio {
            addr: parse_mdio_addr(device_name),
        }),
        "scsi" => {
            let (host, channel, target, lun) = parse_scsi(device_name)?;
            Some(Bus::Scsi {
                nr: i16::try_from(host).ok()?,
                addr: pack_scsi_addr(channel, target, lun)?,
            })
        }
        "sdio" => {
            let (nr, slot, function) = parse_sdio(device_name)?;
            Some(Bus::Sdio {
                nr,
                addr: pack_sdio_addr(slot, function)?,
            })
        }
        _ => None,
    }
}

/// An i2c device is really on the ISA bus when the bus number is the legacy marker, or when the
/// adapter it hangs off names itself an ISA adapter.
async fn classify_i2c(
    sysfs_root: &Path,
    nr: i16,
    addr: u32,
    reader: &IdentityReader,
) -> Result<Bus, Stalled> {
    if nr == I2C_ISA_BUS_NR {
        return Ok(Bus::Isa { addr });
    }
    let adapter_name_path = sysfs_root.join(format!("class/i2c-adapter/i2c-{nr}/device/name"));
    let is_isa_adapter = reader
        .text(&adapter_name_path)
        .await?
        .is_some_and(|name| name.starts_with(I2C_ISA_ADAPTER_PREFIX));
    if is_isa_adapter {
        Ok(Bus::Isa { addr })
    } else {
        Ok(Bus::I2c { nr, addr })
    }
}

/// PCI address packing from `classify_device`. The widths are the PCI spec's own, so a name that
/// overflows one of them is not a PCI address and is left for the next classifier.
fn pack_pci_addr(parts: (u32, u32, u32, u32)) -> Option<u32> {
    let (domain, bus, slot, function) = parts;
    if domain > 0xFFFF || bus > 0xFF || slot > 0x1F || function > 0x7 {
        return None;
    }
    Some((domain << 16) + (bus << 8) + (slot << 3) + function)
}

/// SCSI address packing: channel, target, lun. Widths bound each field to its own nibble or byte
/// so that one cannot bleed into the next.
fn pack_scsi_addr(channel: u32, target: u32, lun: u32) -> Option<u32> {
    if channel > 0xFF || target > 0xF || lun > 0xF {
        return None;
    }
    Some((channel << 8) + (target << 4) + lun)
}

/// SDIO address packing: address and function. `Display` unpacks it again.
fn pack_sdio_addr(slot: u32, function: u32) -> Option<u32> {
    if slot > 0xFFFF || function > 0x7 {
        return None;
    }
    Some((slot << 3) + function)
}

/// `%hd-%x`, an i2c bus number and address.
fn parse_i2c(device_name: &str) -> Option<(i16, u32)> {
    let (nr, rest) = split_radix(device_name, 10)?;
    let (addr, _) = split_radix(rest.strip_prefix('-')?, 16)?;
    Some((i16::try_from(nr).ok()?, addr))
}

/// `spi%hd.%d`, note the decimal address.
fn parse_spi(device_name: &str) -> Option<(i16, u32)> {
    let (nr, rest) = split_radix(device_name.strip_prefix("spi")?, 10)?;
    let (addr, _) = split_radix(rest.strip_prefix('.')?, 10)?;
    Some((i16::try_from(nr).ok()?, addr))
}

/// `%x:%x:%x.%x`, shared by the PCI and HID forms which differ only in what the fields mean.
fn parse_quad_hex(device_name: &str) -> Option<(u32, u32, u32, u32)> {
    let (first, rest) = split_radix(device_name, 16)?;
    let (second, rest) = split_radix(rest.strip_prefix(':')?, 16)?;
    let (third, rest) = split_radix(rest.strip_prefix(':')?, 16)?;
    let (fourth, _) = split_radix(rest.strip_prefix('.')?, 16)?;
    Some((first, second, third, fourth))
}

/// `%d:%d:%d:%x`, a SCSI host, channel, target and lun.
fn parse_scsi(device_name: &str) -> Option<(u32, u32, u32, u32)> {
    let (host, rest) = split_radix(device_name, 10)?;
    let (channel, rest) = split_radix(rest.strip_prefix(':')?, 10)?;
    let (target, rest) = split_radix(rest.strip_prefix(':')?, 10)?;
    let (lun, _) = split_radix(rest.strip_prefix(':')?, 16)?;
    Some((host, channel, target, lun))
}

/// `mmc%hx:%x:%x`, an mmc host, address and function.
fn parse_sdio(device_name: &str) -> Option<(i16, u32, u32)> {
    let (nr, rest) = split_radix(device_name.strip_prefix("mmc")?, 16)?;
    let (slot, rest) = split_radix(rest.strip_prefix(':')?, 16)?;
    let (function, _) = split_radix(rest.strip_prefix(':')?, 16)?;
    Some((i16::try_from(nr).ok()?, slot, function))
}

/// The ISA address of a platform device, which has two accepted spellings and a default of 0.
///
/// `%*[a-zA-Z0-9_-]%*1[.:]%d` is a driver name, one separator and a **decimal** address, so
/// `nct6687.2592` is 0x0a20. Device tree names instead carry a hex address, `1c500000.i2c`,
/// which the fallback `%x.%*s` picks up.
fn parse_isa_addr(device_name: &str) -> u32 {
    let name_len = device_name
        .find(|c: char| is_device_name_char(c).not())
        .unwrap_or(device_name.len());
    if name_len > 0 {
        if let Some(rest) = device_name[name_len..].strip_prefix(['.', ':']) {
            if let Some((addr, _)) = split_radix(rest, 10) {
                return addr;
            }
        }
    }
    split_radix(device_name, 16).map_or(0, |(addr, _)| addr)
}

/// `%*[^:]:%d`, the decimal address of an mdio device.
fn parse_mdio_addr(device_name: &str) -> u32 {
    let Some(colon_index) = device_name.find(':') else {
        return 0;
    };
    // The skipped run must match at least one character.
    if colon_index == 0 {
        return 0;
    }
    split_radix(&device_name[colon_index + 1..], 10).map_or(0, |(addr, _)| addr)
}

fn is_device_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Splits off a leading run of digits in `radix`, returning it and the remainder.
///
/// Stands in for the scanf conversions above: like scanf it ignores what follows, unlike scanf it
/// rejects rather than wraps on overflow, and it does not accept a sign. Neither appears in a
/// sysfs device name.
fn split_radix(text: &str, radix: u32) -> Option<(u32, &str)> {
    debug_assert!(radix == 10 || radix == 16);
    let digits_len = text
        .find(|c: char| c.is_digit(radix).not())
        .unwrap_or(text.len());
    if digits_len == 0 {
        return None;
    }
    let (digits, rest) = text.split_at(digits_len);
    u32::from_str_radix(digits, radix)
        .ok()
        .map(|value| (value, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Builds a throwaway sysfs tree. Devices live under `devices/`, hwmon directories under
    /// `class/hwmon/`, and the links between them are real symlinks so that the symlink-only
    /// resolution rules are actually exercised.
    struct SysfsFixture {
        root: TempDir,
    }

    impl SysfsFixture {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().unwrap(),
            }
        }

        fn path(&self) -> &Path {
            self.root.path()
        }

        /// Creates a device directory with an optional subsystem link, returning its path.
        fn device(&self, relative_path: &str, subsystem: Option<&str>) -> PathBuf {
            let device_path = self.root.path().join("devices").join(relative_path);
            fs::create_dir_all(&device_path).unwrap();
            if let Some(subsystem) = subsystem {
                let subsystem_path = self.root.path().join("bus").join(subsystem);
                fs::create_dir_all(&subsystem_path).unwrap();
                std::os::unix::fs::symlink(&subsystem_path, device_path.join("subsystem")).unwrap();
            }
            device_path
        }

        /// Creates an hwmon directory with a `name` attribute, linked to `device_path`.
        fn hwmon(&self, number: u32, name: &str, device_path: Option<&Path>) -> PathBuf {
            let hwmon_path = self
                .root
                .path()
                .join("class/hwmon")
                .join(format!("hwmon{number}"));
            fs::create_dir_all(&hwmon_path).unwrap();
            fs::write(hwmon_path.join("name"), format!("{name}\n")).unwrap();
            if let Some(device_path) = device_path {
                std::os::unix::fs::symlink(device_path, hwmon_path.join("device")).unwrap();
            }
            hwmon_path
        }

        /// Links `child` up to `parent` the way sysfs does for a nested device.
        fn set_parent(child: &Path, parent: &Path) {
            std::os::unix::fs::symlink(parent, child.join("device")).unwrap();
        }

        /// Creates the i2c adapter whose `name` decides whether a bus is really ISA.
        fn i2c_adapter(&self, nr: i16, name: &str) {
            let adapter_path = self
                .root
                .path()
                .join(format!("class/i2c-adapter/i2c-{nr}/device"));
            fs::create_dir_all(&adapter_path).unwrap();
            fs::write(adapter_path.join("name"), format!("{name}\n")).unwrap();
        }

        async fn derive(&self, hwmon_path: &Path) -> Option<String> {
            derive_in(self.path(), hwmon_path, &IdentityReader::inline())
                .await
                .expect("an inline read always answers")
                .map(|chip| chip.to_string())
        }
    }

    /// Goal: the bus types we can hit in the field must produce byte-identical names to
    /// `sensors`. Method: build one fixture tree per bus type, each named after a device that
    /// really exists, and assert the rendered name. The expected values come from the
    /// verification run against the real machine.
    #[test]
    fn derives_names_for_every_bus_type() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let cases: Vec<(u32, &str, &str, Option<&str>, &str)> = vec![
                // (hwmon number, chip name, device dir, subsystem, expected)
                (
                    0,
                    "k10temp",
                    "pci0000:00/0000:00:18.3",
                    Some("pci"),
                    "k10temp-pci-00c3",
                ),
                (
                    1,
                    "nct6687",
                    "platform/nct6687.2592",
                    Some("platform"),
                    "nct6687-isa-0a20",
                ),
                (
                    2,
                    "nzxtsmart2",
                    "hid/0003:1E71:2010.000D",
                    Some("hid"),
                    "nzxtsmart2-hid-3-d",
                ),
                (3, "lm75", "i2c/12-004b", Some("i2c"), "lm75-i2c-12-4b"),
                (
                    4,
                    "w83627hf",
                    "i2c/9191-0290",
                    Some("i2c"),
                    "w83627hf-isa-0290",
                ),
                (5, "max31855", "spi/spi0.1", Some("spi"), "max31855-spi-0-1"),
                (
                    6,
                    "drivetemp",
                    "scsi/1:0:0:0",
                    Some("scsi"),
                    "drivetemp-scsi-1-0",
                ),
                (
                    7,
                    "acpitz",
                    "acpi/LNXTHERM:00",
                    Some("acpi"),
                    "acpitz-acpi-0",
                ),
                (8, "sfp", "mdio/gpio-0:04", Some("mdio_bus"), "sfp-mdio-4"),
                (
                    9,
                    "mmc_dev",
                    "sdio/mmc1:0001:2",
                    Some("sdio"),
                    "mmc_dev-sdio-1:0001:2",
                ),
                (
                    10,
                    "coretemp",
                    "platform/coretemp.0",
                    Some("platform"),
                    "coretemp-isa-0000",
                ),
                // Device tree names carry a hex address instead of a decimal one.
                (
                    11,
                    "sun4i_ts",
                    "platform/1c25000.rtp",
                    Some("of_platform"),
                    "sun4i_ts-isa-1c25000",
                ),
            ];
            for (number, name, device, subsystem, expected) in cases {
                let device_path = fixture.device(device, subsystem);
                let hwmon_path = fixture.hwmon(number, name, Some(&device_path));
                assert_eq!(
                    fixture.derive(&hwmon_path).await.as_deref(),
                    Some(expected),
                    "wrong chip name for {name} at {device}"
                );
            }
        });
    }

    /// Goal: a device whose own subsystem means nothing to us must still be named after the bus
    /// it hangs off, which is how `nvme` chips get a PCI name. Method: an `nvme` device with an
    /// unrecognized subsystem whose parent is a PCI device.
    #[test]
    fn walks_up_to_the_first_recognized_parent() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let pci_path = fixture.device("pci0000:00/0000:0c:00.0", Some("pci"));
            let nvme_path = fixture.device("pci0000:00/0000:0c:00.0/nvme/nvme0", Some("nvme"));
            SysfsFixture::set_parent(&nvme_path, &pci_path);
            let hwmon_path = fixture.hwmon(0, "nvme", Some(&nvme_path));

            assert_eq!(
                fixture.derive(&hwmon_path).await.as_deref(),
                Some("nvme-pci-0c00")
            );
        });
    }

    /// Goal: the walk must terminate on a tree that never classifies, rather than spin. Method: a
    /// chain of unrecognized devices longer than the walk limit. Failure mode is a hung test.
    #[test]
    fn bounds_the_parent_walk() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let depth = MAX_PARENT_WALK + 4;
            let mut previous = fixture.device("chain/link0", Some("unrecognized"));
            for index in 1..depth {
                let current = fixture.device(&format!("chain/link{index}"), Some("unrecognized"));
                SysfsFixture::set_parent(&previous, &current);
                previous = current;
            }
            let first = fixture.path().join("devices/chain/link0");
            let hwmon_path = fixture.hwmon(0, "deep", Some(&first));

            // Unclassifiable, so it falls back to virtual.
            assert_eq!(
                fixture.derive(&hwmon_path).await.as_deref(),
                Some("deep-virtual-0")
            );
        });
    }

    /// Goal: the three ways a device ends up virtual must all render as `-virtual-0`. Method: no
    /// device link at all, a device link that is a plain directory rather than a symlink, and a
    /// chain whose top has no parent left to walk to.
    #[test]
    fn treats_unresolvable_devices_as_virtual() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let no_link = fixture.hwmon(0, "amdgpu", None);
            assert_eq!(
                fixture.derive(&no_link).await.as_deref(),
                Some("amdgpu-virtual-0")
            );

            let not_a_symlink = fixture.hwmon(1, "thinkpad", None);
            fs::create_dir_all(not_a_symlink.join("device")).unwrap();
            assert_eq!(
                fixture.derive(&not_a_symlink).await.as_deref(),
                Some("thinkpad-virtual-0")
            );

            let orphan = fixture.device("odd/device0", Some("unrecognized"));
            let dead_end = fixture.hwmon(2, "odd", Some(&orphan));
            assert_eq!(
                fixture.derive(&dead_end).await.as_deref(),
                Some("odd-virtual-0")
            );
        });
    }

    /// Goal: a device with no subsystem link must be classified as ISA, not walked up, because
    /// libsensors' platform branch accepts a missing subsystem. Method: a device directory with
    /// no `subsystem` link and a platform style name.
    #[test]
    fn classifies_a_missing_subsystem_as_isa() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let device_path = fixture.device("platform/it8686.2624", None);
            let hwmon_path = fixture.hwmon(0, "it8686", Some(&device_path));

            assert_eq!(
                fixture.derive(&hwmon_path).await.as_deref(),
                Some("it8686-isa-0a40")
            );
        });
    }

    /// Goal: an i2c bus that is really an ISA adapter must be named ISA, which is how some
    /// Super-I/O chips appear. Method: two identical i2c devices, one whose adapter names itself
    /// an ISA adapter and one whose adapter does not.
    #[test]
    fn detects_an_isa_adapter_behind_an_i2c_bus() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            fixture.i2c_adapter(5, "ISA adapter");
            fixture.i2c_adapter(6, "SMBus PIIX4 adapter");

            let isa_device = fixture.device("i2c/5-0290", Some("i2c"));
            let isa_hwmon = fixture.hwmon(0, "w83627", Some(&isa_device));
            assert_eq!(
                fixture.derive(&isa_hwmon).await.as_deref(),
                Some("w83627-isa-0290")
            );

            let smbus_device = fixture.device("i2c/6-002d", Some("i2c"));
            let smbus_hwmon = fixture.hwmon(1, "lm85", Some(&smbus_device));
            assert_eq!(
                fixture.derive(&smbus_hwmon).await.as_deref(),
                Some("lm85-i2c-6-2d")
            );
        });
    }

    /// Goal: `CentOS` style paths, which end in `device` and are the device directory themselves,
    /// must classify off that directory rather than looking for a `device` link inside it.
    /// Method: an hwmon path pointing straight at a PCI device directory.
    #[test]
    fn derives_from_a_centos_style_path() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let device_path = fixture.device("pci0000:00/0000:00:18.3", Some("pci"));
            // The name attribute lives on the device directory for these paths.
            fs::write(device_path.join("name"), "k10temp\n").unwrap();
            let hwmon_path = fixture.path().join("class/hwmon/hwmon0/device");
            fs::create_dir_all(hwmon_path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&device_path, &hwmon_path).unwrap();

            assert_eq!(
                fixture.derive(&hwmon_path).await.as_deref(),
                Some("k10temp-pci-00c3")
            );
        });
    }

    /// Goal: a directory that is not a chip must not produce a name, since libsensors ignores any
    /// device without a `name` attribute. Method: a missing `name` file and an empty one.
    #[test]
    fn rejects_a_directory_without_a_name() {
        crate::rt::test_runtime(async {
            let fixture = SysfsFixture::new();
            let missing = fixture.path().join("class/hwmon/hwmon0");
            fs::create_dir_all(&missing).unwrap();
            assert_eq!(fixture.derive(&missing).await, None);

            let empty = fixture.hwmon(1, "", None);
            assert_eq!(fixture.derive(&empty).await, None);
        });
    }

    /// Goal: the address parsers must follow scanf's rules, including the ones that surprise.
    /// Method: exercise each parser directly on the forms that decide a name, positive and
    /// negative.
    #[test]
    fn parses_addresses_the_way_scanf_does() {
        // A platform address is decimal, so the printed hex differs from the digits.
        assert_eq!(parse_isa_addr("nct6687.2592"), 0x0a20);
        assert_eq!(parse_isa_addr("it8686.2624"), 0x0a40);
        assert_eq!(parse_isa_addr("nct6775.656"), 0x0290);
        // A colon separates just as well as a dot.
        assert_eq!(parse_isa_addr("PNP0C14:02"), 2);
        // Device tree names fall through to the hex form.
        assert_eq!(parse_isa_addr("1c500000.i2c"), 0x1c50_0000);
        // Nothing address-like at all defaults to zero rather than failing.
        assert_eq!(parse_isa_addr("thinkpad_hwmon"), 0);

        assert_eq!(parse_i2c("12-004b"), Some((12, 0x4b)));
        assert_eq!(parse_i2c("9191-0290"), Some((9191, 0x290)));
        // A PCI name must not parse as i2c, or every PCI chip would be misnamed.
        assert_eq!(parse_i2c("0000:00:18.3"), None);

        assert_eq!(parse_spi("spi0.1"), Some((0, 1)));
        assert_eq!(parse_spi("spi32766.10"), Some((32766, 10)));
        assert_eq!(parse_spi("0.1"), None);

        assert_eq!(parse_quad_hex("0000:00:18.3"), Some((0, 0, 0x18, 3)));
        assert_eq!(
            parse_quad_hex("0003:1E71:2010.000D"),
            Some((3, 0x1e71, 0x2010, 0xd))
        );
        assert_eq!(parse_quad_hex("0000:00:18"), None);

        assert_eq!(parse_scsi("1:0:0:0"), Some((1, 0, 0, 0)));
        assert_eq!(parse_scsi("6:0:1:15"), Some((6, 0, 1, 0x15)));

        assert_eq!(parse_sdio("mmc1:0001:2"), Some((1, 1, 2)));
        assert_eq!(parse_sdio("1:0001:2"), None);

        assert_eq!(parse_mdio_addr("gpio-0:04"), 4);
        assert_eq!(parse_mdio_addr("nocolon"), 0);
        assert_eq!(parse_mdio_addr(":04"), 0);
    }

    /// Goal: the packers must reject values that would silently corrupt a neighbouring field,
    /// since a wrong address is worse than no name. Method: each field at its limit and one past.
    #[test]
    fn rejects_addresses_that_overflow_their_field() {
        assert_eq!(pack_pci_addr((0, 0, 0x18, 3)), Some(0x00c3));
        assert_eq!(pack_pci_addr((0xFFFF, 0xFF, 0x1F, 0x7)), Some(0xFFFF_FFFF));
        assert_eq!(pack_pci_addr((0x1_0000, 0, 0, 0)), None);
        assert_eq!(pack_pci_addr((0, 0, 0x20, 0)), None);
        assert_eq!(pack_pci_addr((0, 0, 0, 0x8)), None);

        assert_eq!(pack_scsi_addr(0, 0, 0), Some(0));
        assert_eq!(pack_scsi_addr(0xFF, 0xF, 0xF), Some(0xFFFF));
        assert_eq!(pack_scsi_addr(0x100, 0, 0), None);
        assert_eq!(pack_scsi_addr(0, 0x10, 0), None);

        assert_eq!(pack_sdio_addr(1, 2), Some(0xa));
        assert_eq!(pack_sdio_addr(0x1_0000, 0), None);
        assert_eq!(pack_sdio_addr(0, 0x8), None);
    }

    /// Goal: every bus type must render exactly as `sensors_snprintf_chip_name` does, including
    /// the zero padding, which the fixture tests only cover for the types they reach. Method:
    /// render one of each directly.
    #[test]
    fn renders_every_bus_type() {
        let render = |bus| {
            ChipName {
                prefix: "chip".to_owned(),
                bus,
            }
            .to_string()
        };
        assert_eq!(render(Bus::Isa { addr: 0x290 }), "chip-isa-0290");
        assert_eq!(render(Bus::Pci { addr: 0xc3 }), "chip-pci-00c3");
        assert_eq!(render(Bus::I2c { nr: 0, addr: 0x4b }), "chip-i2c-0-4b");
        assert_eq!(render(Bus::Spi { nr: 1, addr: 0x1f }), "chip-spi-1-1f");
        assert_eq!(render(Bus::Virtual), "chip-virtual-0");
        assert_eq!(render(Bus::Acpi), "chip-acpi-0");
        assert_eq!(render(Bus::Hid { nr: 3, addr: 0xd }), "chip-hid-3-d");
        assert_eq!(render(Bus::Mdio { addr: 4 }), "chip-mdio-4");
        assert_eq!(render(Bus::Scsi { nr: 1, addr: 0 }), "chip-scsi-1-0");
        assert_eq!(
            render(Bus::Sdio {
                nr: 1,
                addr: (1 << 3) + 2
            }),
            "chip-sdio-1:0001:2"
        );
    }
}
