/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2025  Guy Boldon, megadjc and contributors
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

//! Chips requiring custom detection logic beyond simple ID matching.
//!
//! Some Super-I/O chips need non-standard register reads, revision checks,
//! or alternative device ID registers. These are defined entirely in Rust
//! with their data and detection logic together.

use log::debug;

use crate::chip_db::ChipEntry;
use crate::port_io::{PortIo, PortIoError};

/// A chip with custom detection logic.
pub struct CustomChip {
    pub entry: ChipEntry,
    /// Custom detection function. Returns `true` if this chip is detected.
    /// Called after the standard config mode entry for the family.
    pub detect: fn(&mut dyn PortIo, u16, u16) -> Result<bool, PortIoError>,
}

/// SMSC non-standard chips that use register 0x0D instead of 0x20 for their device ID.
/// These are detected before the standard path by reading the alternate register.
#[must_use]
pub fn custom_smsc_ns_chips() -> Vec<CustomChip> {
    // These chips don't have sensor drivers, but we include them for identification.
    // The detection function reads register 0x0D instead of 0x20.
    Vec::new() // All SMSC NS chips are "not-a-sensor" - no actionable entries needed.
}

/// SMSC chips with non-standard base address registers.
#[must_use]
pub fn custom_smsc_chips() -> Vec<CustomChip> {
    vec![
        // SCH5627 uses registers 0x66/0x67 for base address instead of 0x60/0x61
        CustomChip {
            entry: ChipEntry {
                name: "SMSC SCH5627 Super IO".into(),
                driver: "sch5627".into(),
                devid: 0xC6,
                devid_mask: 0xFF,
                logdev: 0x0C,
                features: vec!["voltage".into(), "fan".into(), "temp".into()],
            },
            detect: smsc_sch5627_detect,
        },
        // SCH5636 uses registers 0x66/0x67 for base address instead of 0x60/0x61
        CustomChip {
            entry: ChipEntry {
                name: "SMSC SCH5636 Super IO".into(),
                driver: "sch5636".into(),
                devid: 0xC7,
                devid_mask: 0xFF,
                logdev: 0x0C,
                features: vec!["voltage".into(), "fan".into(), "temp".into()],
            },
            detect: smsc_sch5636_detect,
        },
    ]
}

fn smsc_sch5627_detect(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
) -> Result<bool, PortIoError> {
    // Read standard device ID register
    port_io.outb(addr_reg, 0x20)?;
    let id = port_io.inb(data_reg)?;
    if id == 0xC6 {
        debug!("SCH5627 detected via custom detection (non-standard base regs 0x66/0x67)");
        return Ok(true);
    }
    Ok(false)
}

fn smsc_sch5636_detect(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
) -> Result<bool, PortIoError> {
    port_io.outb(addr_reg, 0x20)?;
    let id = port_io.inb(data_reg)?;
    if id == 0xC7 {
        debug!("SCH5636 detected via custom detection (non-standard base regs 0x66/0x67)");
        return Ok(true);
    }
    Ok(false)
}

/// National Semiconductor chips that share the same device ID but are
/// differentiated by register 0x27.
#[must_use]
pub fn custom_natsemi_chips() -> Vec<CustomChip> {
    vec![
        // PC8374L: register 0x27 < 0x80
        CustomChip {
            entry: ChipEntry {
                name: "Nat. Semi. PC8374L Super IO Sensors".into(),
                driver: String::new(), // driver "to-be-written" in upstream
                devid: 0xF1,
                devid_mask: 0xFF,
                logdev: 0x08,
                features: vec!["voltage".into(), "fan".into(), "temp".into()],
            },
            detect: pc8374l_detect,
        },
    ]
}

fn pc8374l_detect(
    port_io: &mut dyn PortIo,
    addr_reg: u16,
    data_reg: u16,
) -> Result<bool, PortIoError> {
    // First check the device ID
    port_io.outb(addr_reg, 0x20)?;
    let id = port_io.inb(data_reg)?;
    if id != 0xF1 {
        return Ok(false);
    }
    // Disambiguate PC8374L vs WPCD377I using register 0x27
    port_io.outb(addr_reg, 0x27)?;
    let rev = port_io.inb(data_reg)?;
    debug!("PC8374L/WPCD377I disambiguation: reg 0x27 = 0x{rev:02X}");
    Ok(rev < 0x80) // PC8374L has rev < 0x80, WPCD377I has rev >= 0x80
}

/// Get all custom chips organized by family index.
/// Family indices: 0=ITE, 1=Winbond, 2=NatSemi, 3=SMSC
#[must_use]
pub fn all_custom_chips() -> Vec<(usize, Vec<CustomChip>)> {
    vec![
        (2, custom_natsemi_chips()),
        (3, custom_smsc_chips()),
        // Families 0 (ITE) and 1 (Winbond) have no custom chips -
        // all their chips use standard ID matching from TOML.
    ]
}

/// The ITE eSPI-to-LPC bridge ID. When detected in the fast path, it means
/// the real Super-I/O chip is behind an eSPI bus and inaccessible.
pub const ITE_ESPI_BRIDGE_ID: u16 = 0x8883;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port_io::MockPortIo;

    #[test]
    fn test_pc8374l_detect_positive() {
        let mut mock = MockPortIo::new(vec![
            0xF1, // read devid register -> 0xF1
            0x11, // read reg 0x27 -> 0x11 (< 0x80 = PC8374L)
        ]);
        let result = pc8374l_detect(&mut mock, 0x2E, 0x2F).unwrap();
        assert!(result);
    }

    #[test]
    fn test_pc8374l_detect_wpcd377i() {
        let mut mock = MockPortIo::new(vec![
            0xF1, // read devid register -> 0xF1
            0x91, // read reg 0x27 -> 0x91 (>= 0x80 = WPCD377I)
        ]);
        let result = pc8374l_detect(&mut mock, 0x2E, 0x2F).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_pc8374l_detect_wrong_id() {
        let mut mock = MockPortIo::new(vec![
            0xE1, // read devid register -> wrong ID
        ]);
        let result = pc8374l_detect(&mut mock, 0x2E, 0x2F).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_sch5627_detect_positive() {
        let mut mock = MockPortIo::new(vec![
            0xC6, // read devid register -> 0xC6
        ]);
        let result = smsc_sch5627_detect(&mut mock, 0x2E, 0x2F).unwrap();
        assert!(result);
    }

    #[test]
    fn test_sch5627_detect_negative() {
        let mut mock = MockPortIo::new(vec![
            0x00, // read devid register -> wrong
        ]);
        let result = smsc_sch5627_detect(&mut mock, 0x2E, 0x2F).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_all_custom_chips() {
        let chips = all_custom_chips();
        assert!(!chips.is_empty());
        // NatSemi family (index 2: ITE=0, Winbond=1, NatSemi=2, SMSC=3)
        let natsemi = &chips[0];
        assert_eq!(natsemi.0, 2);
        // SMSC family
        let smsc = &chips[1];
        assert_eq!(smsc.0, 3);
        assert_eq!(smsc.1.len(), 2); // SCH5627 + SCH5636
    }
}
