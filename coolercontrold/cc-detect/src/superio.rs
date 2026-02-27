/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2026  Guy Boldon, megadjc and contributors
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

//! Super-I/O two-path detection algorithm.
//!
//! Implements the detection approach from the TsunamiMommy/lm-sensors fork:
//! 1. **Fast path (non-invasive)**: Read chip ID without entering config mode
//! 2. **Fallback path**: Enter config mode with family-specific password sequences
//!
//! This prevents hardware issues, especially on Gigabyte motherboards where
//! sending entry sequences can interfere with ITE chips.

use log::{debug, info, warn};

use crate::chip_db::{ChipDatabase, ChipEntry, ChipFamily};
use crate::chips_custom::{all_custom_chips, CustomChip, ITE_ESPI_BRIDGE_ID};
use crate::port_io::{PortIo, PortIoError};

/// Standard Super-I/O register addresses (PNP ISA spec).
const DEVIDREG_HI: u8 = 0x20;
const DEVIDREG_LO: u8 = 0x21;
const LOGDEVREG: u8 = 0x07;
const ACTREG: u8 = 0x30;
const ACTMASK: u8 = 0x01;
const BASEREG_MSB: u8 = 0x60;
const BASEREG_LSB: u8 = 0x61;

/// The I/O port addresses to probe for Super-I/O chips.
pub const SUPERIO_ADDRS: [(u16, u16); 2] = [
    (0x2E, 0x2F), // Primary address/data port pair
    (0x4E, 0x4F), // Secondary address/data port pair
];

/// Information about a detected Super-I/O chip.
#[derive(Debug, Clone)]
pub struct DetectedChip {
    pub name: String,
    pub driver: String,
    pub address: u16,
    pub base_address: u16,
    pub device_id: u16,
    pub features: Vec<String>,
    pub active: bool,
}

/// Run Super-I/O detection on all standard addresses.
pub fn detect_superio(port_io: &mut dyn PortIo, db: &ChipDatabase) -> Vec<DetectedChip> {
    let custom_chips = all_custom_chips();
    let mut detected = Vec::new();

    for &(addr_reg, data_reg) in &SUPERIO_ADDRS {
        debug!("Probing Super-I/O at address 0x{addr_reg:02X}/0x{data_reg:02X}");

        // Fast path: try reading ID without entering config mode
        match try_fast_path(port_io, addr_reg, data_reg, db) {
            Ok(Some(chip)) => {
                info!(
                    "Detected {} at 0x{:02X} (driver: {}) (Fast path)",
                    chip.name, addr_reg, chip.driver
                );
                detected.push(chip);
                continue; // Skip fallback for this address
            }
            Ok(None) => {
                debug!("No chip detected at 0x{addr_reg:02X} (Fast path)");
            }
            Err(err) => {
                debug!("Error at 0x{addr_reg:02X} (Fast path): {err}");
            }
        }

        // Fallback path: try each family with config mode entry
        match try_fallback_path(port_io, addr_reg, data_reg, db, &custom_chips) {
            Ok(Some(chip)) => {
                info!(
                    "Detected {} at 0x{:02X} (driver: {}) (Fallback path)",
                    chip.name, addr_reg, chip.driver
                );
                detected.push(chip);
            }
            Ok(None) => {
                debug!("No chip detected at 0x{addr_reg:02X} (Fallback path)");
            }
            Err(err) => {
                debug!("Error at 0x{addr_reg:02X} (Fallback path): {err}");
            }
        }
    }

    detected
}

/// Fast path: read chip ID without entering config mode (non-invasive).
fn try_fast_path(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    db: &ChipDatabase,
) -> Result<Option<DetectedChip>, PortIoError> {
    let id = read_device_id(port_io, addr_reg, data_reg)?;
    debug!("Fast path read ID at 0x{addr_reg:02X}: 0x{id:04X}");

    if id == 0x0000 || id == 0xFFFF {
        return Ok(None);
    }

    // Check for ITE eSPI-to-LPC Bridge
    if id == ITE_ESPI_BRIDGE_ID {
        warn!(
            "ITE eSPI-to-LPC Bridge detected at 0x{addr_reg:02X}. \
             The Super-I/O chip is behind an eSPI bus and may be \
             inaccessible until the next system restart."
        );
        return Ok(None);
    }

    // Try to match against the database
    if let Some((_, chip)) = db.find_chip(id) {
        if chip.has_driver() {
            let base_addr = read_base_address(port_io, addr_reg, data_reg, chip.logdev)?;
            let active = read_activation(port_io, addr_reg, data_reg, chip.logdev)?;
            return Ok(Some(DetectedChip {
                name: chip.name.clone(),
                driver: chip.driver.clone(),
                address: addr_reg,
                base_address: base_addr,
                device_id: id,
                features: chip.features.clone(),
                active,
            }));
        }
        debug!("Fast path matched {} but no driver available", chip.name);
    }

    Ok(None)
}

/// Fallback path: try each chip family with config mode entry sequences.
fn try_fallback_path(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    db: &ChipDatabase,
    custom_chips: &[(usize, Vec<CustomChip>)],
) -> Result<Option<DetectedChip>, PortIoError> {
    // Ensure clean state before probing
    exit_superio(port_io, addr_reg, data_reg)?;

    for (family_idx, family) in db.families.iter().enumerate() {
        // Get custom chips for this family, if any
        let family_custom_chips: Vec<&CustomChip> = custom_chips
            .iter()
            .filter(|(idx, _)| *idx == family_idx)
            .flat_map(|(_, chips)| chips.iter())
            .collect();

        match probe_family(port_io, addr_reg, data_reg, family, &family_custom_chips) {
            Ok(Some(chip)) => {
                exit_superio(port_io, addr_reg, data_reg)?;
                return Ok(Some(chip));
            }
            Ok(None) => {
                exit_superio(port_io, addr_reg, data_reg)?;
            }
            Err(err) => {
                // Always try to exit config mode even on error
                let _ = exit_superio(port_io, addr_reg, data_reg);
                debug!(
                    "Error probing family {} at 0x{:02X}: {}",
                    family.name, addr_reg, err
                );
            }
        }
    }

    Ok(None)
}

/// Probe a single chip family at the given address.
fn probe_family(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    family: &ChipFamily,
    custom_chips: &[&CustomChip],
) -> Result<Option<DetectedChip>, PortIoError> {
    debug!("Probing family: {} at 0x{:02X}", family.name, addr_reg);

    // Enter config mode with family-specific password sequence
    let entry_seq = family.entry_sequence(addr_reg);
    for &byte in entry_seq {
        port_io.outb(addr_reg, byte)?;
    }

    // Try custom chip detection functions first
    for custom_chip in custom_chips {
        match (custom_chip.detect)(port_io, addr_reg, data_reg) {
            Ok(true) => {
                debug!("Custom chip detected: {}", custom_chip.entry.name);
                if custom_chip.entry.has_driver() {
                    let (base_addr, active) =
                        read_chip_details(port_io, addr_reg, data_reg, &custom_chip.entry)?;
                    return Ok(Some(DetectedChip {
                        name: custom_chip.entry.name.clone(),
                        driver: custom_chip.entry.driver.clone(),
                        address: addr_reg,
                        base_address: base_addr,
                        device_id: custom_chip.entry.devid,
                        features: custom_chip.entry.features.clone(),
                        active,
                    }));
                }
            }
            Ok(false) => {}
            Err(err) => {
                debug!(
                    "Custom detection error for {}: {}",
                    custom_chip.entry.name, err
                );
            }
        }
    }

    // Read the standard device ID
    let id = read_device_id(port_io, addr_reg, data_reg)?;
    debug!("Family {}: read device ID 0x{:04X}", family.name, id);

    if id == 0x0000 || id == 0xFFFF {
        return Ok(None);
    }

    // Match against this family's TOML chips
    for chip in &family.chips {
        if chip.matches_id(id) && chip.has_driver() {
            debug!("Matched chip: {} (driver: {})", chip.name, chip.driver);
            let (base_addr, active) = read_chip_details(port_io, addr_reg, data_reg, chip)?;
            return Ok(Some(DetectedChip {
                name: chip.name.clone(),
                driver: chip.driver.clone(),
                address: addr_reg,
                base_address: base_addr,
                device_id: id,
                features: chip.features.clone(),
                active,
            }));
        }
    }

    debug!("Family {}: no match for ID 0x{:04X}", family.name, id);
    Ok(None)
}

/// Read the 16-bit device ID from registers 0x20 (high) and 0x21 (low).
fn read_device_id(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
) -> Result<u16, PortIoError> {
    port_io.outb(addr_reg, DEVIDREG_HI)?;
    let hi = port_io.inb(data_reg)?;
    port_io.outb(addr_reg, DEVIDREG_LO)?;
    let lo = port_io.inb(data_reg)?;
    Ok(u16::from(hi) << 8 | u16::from(lo))
}

/// Read base address and activation status for a chip.
fn read_chip_details(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    chip: &ChipEntry,
) -> Result<(u16, bool), PortIoError> {
    let base_addr = read_base_address(port_io, addr_reg, data_reg, chip.logdev)?;
    let active = read_activation(port_io, addr_reg, data_reg, chip.logdev)?;
    Ok((base_addr, active))
}

/// Select a logical device and read its I/O base address.
fn read_base_address(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    logdev: u8,
) -> Result<u16, PortIoError> {
    // Select logical device
    port_io.outb(addr_reg, LOGDEVREG)?;
    port_io.outb(data_reg, logdev)?;

    // Read base address (MSB at 0x60, LSB at 0x61)
    port_io.outb(addr_reg, BASEREG_MSB)?;
    let msb = port_io.inb(data_reg)?;
    port_io.outb(addr_reg, BASEREG_LSB)?;
    let lsb = port_io.inb(data_reg)?;
    Ok(u16::from(msb) << 8 | u16::from(lsb))
}

/// Check if a logical device is activated.
fn read_activation(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
    logdev: u8,
) -> Result<bool, PortIoError> {
    // Select logical device
    port_io.outb(addr_reg, LOGDEVREG)?;
    port_io.outb(data_reg, logdev)?;

    // Read activation register
    port_io.outb(addr_reg, ACTREG)?;
    let act = port_io.inb(data_reg)?;
    Ok((act & ACTMASK) != 0)
}

/// Exit Super-I/O config mode.
fn exit_superio(port_io: &mut dyn PortIo, addr_reg: u16, data_reg: u16) -> Result<(), PortIoError> {
    // SMSC/Winbond exit: write 0xAA to address register
    port_io.outb(addr_reg, 0xAA)?;
    // PNP-ISA "Return to Wait For Key" state
    port_io.outb(addr_reg, 0x02)?;
    port_io.outb(data_reg, 0x02)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port_io::MockPortIo;

    fn make_test_db() -> ChipDatabase {
        ChipDatabase::load_compiled()
    }

    /// Create a mock with enough values to run through full detection on both addresses.
    /// Values are returned in sequence for any `inb()` call regardless of port.
    fn mock_for_full_scan(mut reads: Vec<u8>) -> MockPortIo {
        // Pad with 0xFF for any remaining reads (fallback families, 2nd address, etc.)
        reads.resize(200, 0xFF);
        MockPortIo::new(reads)
    }

    #[test]
    fn test_fast_path_detects_ite_chip() {
        // IT8686E has devid 0x8686
        // Fast path reads: hi=0x86, lo=0x86 (valid ITE chip)
        // Then read_base_address: logdev_write, MSB=0x02, LSB=0x90
        // Then read_activation: logdev_write, act=0x01
        let mut mock = mock_for_full_scan(vec![
            0x86, 0x86, // fast path device ID at 0x2E
            0x02, 0x90, // base address
            0x01, // activation
                  // remaining reads for 0x4E and fallback: all 0xFF (padded)
        ]);
        let db = make_test_db();
        let results = detect_superio(&mut mock, &db);

        assert!(!results.is_empty(), "should detect at least one chip");
        let chip = &results[0];
        assert_eq!(chip.driver, "it87");
        assert!(chip.name.contains("IT8686E"));
        assert_eq!(chip.address, 0x2E);
        assert_eq!(chip.base_address, 0x0290);
        assert!(chip.active);
    }

    #[test]
    fn test_fast_path_no_chip() {
        // All reads return 0xFF = no chip anywhere
        let mut mock = mock_for_full_scan(vec![]);
        let db = make_test_db();
        let results = detect_superio(&mut mock, &db);
        assert!(results.is_empty(), "should detect no chips");
    }

    #[test]
    fn test_espi_bridge_warning() {
        let mut mock = mock_for_full_scan(vec![
            0x88,
            0x83, // eSPI bridge ID at 0x2E fast path
                 // Everything else: 0xFF (padded)
        ]);
        let db = make_test_db();
        let results = detect_superio(&mut mock, &db);
        // eSPI bridge should not be reported as a detected chip
        assert!(results.is_empty());
    }

    #[test]
    fn test_read_device_id() {
        let mut mock = MockPortIo::new(vec![0x86, 0x86]);
        let id = read_device_id(&mut mock, 0x2E, 0x2F).unwrap();
        assert_eq!(id, 0x8686);
    }

    #[test]
    fn test_exit_superio_writes() {
        let mut mock = MockPortIo::new(vec![]);
        exit_superio(&mut mock, 0x2E, 0x2F).unwrap();
        assert_eq!(mock.writes, vec![(0x2E, 0xAA), (0x2E, 0x02), (0x2F, 0x02)]);
    }

    #[test]
    fn test_fallback_detects_winbond_chip() {
        // Trace of reads before Winbond family device ID read:
        // Fast path at 0x2E: 2 reads (hi, lo) -> 0x0000 = no chip
        // Fallback:
        //   NatSemi custom (PC8374L): 1 read (devid check fails)
        //   NatSemi standard: 2 reads (hi, lo) -> 0x0000
        //   SMSC custom (SCH5627): 1 read (devid check fails)
        //   SMSC custom (SCH5636): 1 read (devid check fails)
        //   SMSC standard: 2 reads (hi, lo) -> 0x0000
        // Total before Winbond: 9 reads
        // Then Winbond standard: 2 reads -> 0x5200 (W83627HF)
        let mut reads = vec![0x00; 9]; // fast path + NatSemi + SMSC
        reads.extend([0x52, 0x00]); // Winbond family: W83627HF ID (8-bit devid)
        reads.extend([0x02, 0x90]); // base address
        reads.extend([0x01]); // activation
        reads.resize(200, 0xFF);
        let mut mock = MockPortIo::new(reads);
        let db = make_test_db();
        let results = detect_superio(&mut mock, &db);

        assert!(!results.is_empty(), "should detect Winbond chip");
        let chip = &results[0];
        assert_eq!(chip.driver, "w83627hf");
        assert!(chip.name.contains("W83627HF"));
    }
}
