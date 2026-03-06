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

//! TOML chip database loader and chip registry.
//!
//! Loads chip definitions from compiled-in TOML data files and optional
//! runtime override files, providing a unified chip database for detection.

use std::path::Path;

use log::{debug, warn};
use serde::Deserialize;

/// Compiled-in TOML data files.
const TOML_NATIONAL_SEMI: &str = include_str!("../data/superio_national_semi.toml");
const TOML_SMSC: &str = include_str!("../data/superio_smsc.toml");
const TOML_WINBOND: &str = include_str!("../data/superio_winbond.toml");
const TOML_ITE: &str = include_str!("../data/superio_ite.toml");

/// Raw TOML structure for a chip family file.
#[derive(Debug, Deserialize)]
struct TomlChipFamily {
    family: TomlFamilyMeta,
    #[serde(default)]
    chips: Vec<TomlChipEntry>,
}

#[derive(Debug, Deserialize)]
struct TomlFamilyMeta {
    name: String,
    #[serde(default)]
    entry_sequence_2e: Vec<u8>,
    #[serde(default)]
    entry_sequence_4e: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct TomlChipEntry {
    name: String,
    driver: String,
    devid: u16,
    devid_mask: u16,
    logdev: u8,
    #[serde(default)]
    features: Vec<String>,
}

/// A single chip entry in the database.
#[derive(Debug, Clone)]
pub struct ChipEntry {
    pub name: String,
    pub driver: String,
    pub devid: u16,
    pub devid_mask: u16,
    pub logdev: u8,
    pub features: Vec<String>,
}

impl ChipEntry {
    /// Check if this chip's device ID matches a read value.
    #[must_use]
    pub fn matches_id(&self, id: u16) -> bool {
        if self.devid > 0xFF {
            // 16-bit comparison with mask
            (id & self.devid_mask) == self.devid
        } else {
            // 8-bit comparison: only compare high byte of read ID
            (id >> 8) == self.devid
        }
    }

    /// Check if this chip has an actionable kernel driver.
    #[must_use]
    pub fn has_driver(&self) -> bool {
        !self.driver.is_empty()
    }
}

/// A chip family with its entry password sequence and chip entries.
#[derive(Debug, Clone)]
pub struct ChipFamily {
    pub name: String,
    pub entry_sequence_2e: Vec<u8>,
    pub entry_sequence_4e: Vec<u8>,
    pub chips: Vec<ChipEntry>,
}

impl ChipFamily {
    /// Get the entry sequence for the given address port.
    #[must_use]
    pub fn entry_sequence(&self, addr_port: u16) -> &[u8] {
        match addr_port {
            0x2E => &self.entry_sequence_2e,
            0x4E => &self.entry_sequence_4e,
            _ => &[],
        }
    }
}

/// The complete chip database.
#[derive(Debug, Clone)]
pub struct ChipDatabase {
    pub families: Vec<ChipFamily>,
}

impl ChipDatabase {
    /// Load the compiled-in chip database from TOML data files.
    #[must_use]
    pub fn load_compiled() -> Self {
        debug!("Loading compiled chip database");
        let mut families = Vec::new();

        // ITE first: all 16-bit masks; prevents 8-bit SMSC/Winbond entries from
        // false-matching ITE chip IDs (e.g. SMSC devid=0x86 matches 0x86xx ITE IDs).
        // Order: ITE (idx 0), Winbond (idx 1), NatSemi (idx 2), SMSC (idx 3).
        for (toml_str, source) in [
            (TOML_ITE, "ite"),
            (TOML_WINBOND, "winbond"),
            (TOML_NATIONAL_SEMI, "national_semi"),
            (TOML_SMSC, "smsc"),
        ] {
            match parse_toml_family(toml_str) {
                Ok(family) => {
                    debug!(
                        "Loaded {} chips from {} family ({})",
                        family.chips.len(),
                        family.name,
                        source
                    );
                    families.push(family);
                }
                Err(err) => {
                    warn!("Failed to parse compiled chip data for {source}: {err}");
                }
            }
        }

        debug!(
            "Chip database loaded: {} families, {} total chips",
            families.len(),
            families.iter().map(|f| f.chips.len()).sum::<usize>()
        );
        Self { families }
    }

    /// Load and merge an optional override file from the given path.
    /// Entries with matching `devid` replace compiled defaults.
    /// New entries are appended to their chip family.
    /// Invalid entries are logged as warnings and skipped.
    pub fn load_override(&mut self, path: &Path) {
        debug!("Loading override file: {}", path.display());
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(err) => {
                debug!("Override file not found or unreadable: {err}");
                return;
            }
        };

        match parse_toml_family(&content) {
            Ok(override_family) => {
                self.merge_family(override_family);
            }
            Err(err) => {
                warn!("Failed to parse override file {}: {err}", path.display());
            }
        }
    }

    fn merge_family(&mut self, override_family: ChipFamily) {
        // Find the matching family by name
        let existing = self
            .families
            .iter_mut()
            .find(|f| f.name == override_family.name);

        if let Some(family) = existing {
            for override_chip in override_family.chips {
                // Replace existing chip with matching devid, or append
                if let Some(existing_chip) = family
                    .chips
                    .iter_mut()
                    .find(|c| c.devid == override_chip.devid)
                {
                    debug!(
                        "Override: replacing chip {} (devid 0x{:04X})",
                        existing_chip.name, existing_chip.devid
                    );
                    *existing_chip = override_chip;
                } else {
                    debug!(
                        "Override: adding new chip {} (devid 0x{:04X})",
                        override_chip.name, override_chip.devid
                    );
                    family.chips.push(override_chip);
                }
            }
        } else {
            debug!(
                "Override: adding new family {} with {} chips",
                override_family.name,
                override_family.chips.len()
            );
            self.families.push(override_family);
        }
    }

    /// Find a chip matching the given 16-bit device ID across all families.
    /// Prefers 16-bit (more specific) matches over 8-bit matches.
    /// Returns the family index and chip entry reference.
    #[must_use]
    pub fn find_chip(&self, id: u16) -> Option<(usize, &ChipEntry)> {
        // First pass: 16-bit matches only (more specific)
        for (family_idx, family) in self.families.iter().enumerate() {
            for chip in &family.chips {
                if chip.devid > 0xFF && chip.matches_id(id) {
                    return Some((family_idx, chip));
                }
            }
        }
        // Second pass: 8-bit matches (less specific)
        for (family_idx, family) in self.families.iter().enumerate() {
            for chip in &family.chips {
                if chip.devid <= 0xFF && chip.matches_id(id) {
                    return Some((family_idx, chip));
                }
            }
        }
        None
    }

    /// Find all chips matching the given ID in a specific family.
    #[must_use]
    pub fn find_chips_in_family(&self, family_idx: usize, id: u16) -> Vec<&ChipEntry> {
        self.families
            .get(family_idx)
            .map(|family| family.chips.iter().filter(|c| c.matches_id(id)).collect())
            .unwrap_or_default()
    }
}

fn parse_toml_family(toml_str: &str) -> Result<ChipFamily, String> {
    let raw: TomlChipFamily = toml::from_str(toml_str).map_err(|e| e.to_string())?;

    let chips = raw
        .chips
        .into_iter()
        .map(|c| ChipEntry {
            name: c.name,
            driver: c.driver,
            devid: c.devid,
            devid_mask: c.devid_mask,
            logdev: c.logdev,
            features: c.features,
        })
        .collect();

    Ok(ChipFamily {
        name: raw.family.name,
        entry_sequence_2e: raw.family.entry_sequence_2e,
        entry_sequence_4e: raw.family.entry_sequence_4e,
        chips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_compiled_database() {
        let db = ChipDatabase::load_compiled();
        assert_eq!(db.families.len(), 4, "should have 4 families");
        let total_chips: usize = db.families.iter().map(|f| f.chips.len()).sum();
        assert!(
            total_chips > 50,
            "should have >50 total chips, got {total_chips}"
        );
    }

    #[test]
    fn test_parse_national_semi() {
        let family = parse_toml_family(TOML_NATIONAL_SEMI).unwrap();
        assert_eq!(family.name, "National Semiconductor");
        assert!(family.entry_sequence_2e.is_empty());
        assert!(family.entry_sequence_4e.is_empty());
        assert!(!family.chips.is_empty());
    }

    #[test]
    fn test_parse_smsc() {
        let family = parse_toml_family(TOML_SMSC).unwrap();
        assert_eq!(family.name, "SMSC");
        assert_eq!(family.entry_sequence_2e, vec![0x55]);
        assert_eq!(family.entry_sequence_4e, vec![0x55]);
    }

    #[test]
    fn test_parse_winbond() {
        let family = parse_toml_family(TOML_WINBOND).unwrap();
        assert_eq!(family.name, "Winbond / Nuvoton / Fintek");
        assert_eq!(family.entry_sequence_2e, vec![0x87, 0x87]);
    }

    #[test]
    fn test_parse_ite() {
        let family = parse_toml_family(TOML_ITE).unwrap();
        assert_eq!(family.name, "ITE");
        assert_eq!(family.entry_sequence_2e, vec![0x87, 0x01, 0x55, 0x55]);
        assert_eq!(family.entry_sequence_4e, vec![0x87, 0x01, 0x55, 0xAA]);
    }

    #[test]
    fn test_chip_id_matching_16bit() {
        let chip = ChipEntry {
            name: "Test".into(),
            driver: "test".into(),
            devid: 0x8686,
            devid_mask: 0xFFFF,
            logdev: 0x04,
            features: vec![],
        };
        assert!(chip.matches_id(0x8686));
        assert!(!chip.matches_id(0x8687));
    }

    #[test]
    fn test_chip_id_matching_masked() {
        let chip = ChipEntry {
            name: "Test".into(),
            driver: "test".into(),
            devid: 0xC560,
            devid_mask: 0xFFF8,
            logdev: 0x0B,
            features: vec![],
        };
        assert!(chip.matches_id(0xC560));
        assert!(chip.matches_id(0xC561));
        assert!(chip.matches_id(0xC567));
        assert!(!chip.matches_id(0xC568));
    }

    #[test]
    fn test_chip_id_matching_8bit() {
        let chip = ChipEntry {
            name: "Test".into(),
            driver: "test".into(),
            devid: 0xE1,
            devid_mask: 0xFF,
            logdev: 0x09,
            features: vec![],
        };
        // 8-bit match compares high byte of read ID
        assert!(chip.matches_id(0xE100));
        assert!(chip.matches_id(0xE1FF));
        assert!(!chip.matches_id(0xE200));
    }

    #[test]
    fn test_find_chip() {
        let db = ChipDatabase::load_compiled();
        // IT8686E should be found
        let result = db.find_chip(0x8686);
        assert!(result.is_some(), "IT8686E should be in the database");
        let (_, chip) = result.unwrap();
        assert_eq!(chip.driver, "it87");
    }

    #[test]
    fn test_override_replace() {
        let mut db = ChipDatabase::load_compiled();
        let override_toml = r#"
[family]
name = "ITE"
entry_sequence_2e = [0x87, 0x01, 0x55, 0x55]
entry_sequence_4e = [0x87, 0x01, 0x55, 0xAA]

[[chips]]
name = "ITE IT8686E Custom"
driver = "custom_driver"
devid = 0x8686
devid_mask = 0xFFFF
logdev = 0x04
features = ["fan"]
"#;
        let override_family = parse_toml_family(override_toml).unwrap();
        db.merge_family(override_family);

        let (_, chip) = db.find_chip(0x8686).unwrap();
        assert_eq!(chip.driver, "custom_driver");
        assert_eq!(chip.name, "ITE IT8686E Custom");
    }

    #[test]
    fn test_override_add_new() {
        let mut db = ChipDatabase::load_compiled();
        let override_toml = r#"
[family]
name = "ITE"
entry_sequence_2e = [0x87, 0x01, 0x55, 0x55]
entry_sequence_4e = [0x87, 0x01, 0x55, 0xAA]

[[chips]]
name = "ITE IT9999X New Chip"
driver = "it87"
devid = 0x9999
devid_mask = 0xFFFF
logdev = 0x04
features = ["fan", "temp"]
"#;
        let override_family = parse_toml_family(override_toml).unwrap();
        db.merge_family(override_family);

        let result = db.find_chip(0x9999);
        assert!(result.is_some(), "new chip should be added");
        let (_, chip) = result.unwrap();
        assert_eq!(chip.name, "ITE IT9999X New Chip");
    }

    #[test]
    fn test_override_new_family() {
        let mut db = ChipDatabase::load_compiled();
        let override_toml = r#"
[family]
name = "Custom"
entry_sequence_2e = [0xAA]
entry_sequence_4e = [0xBB]

[[chips]]
name = "Custom Chip"
driver = "custom"
devid = 0xAAAA
devid_mask = 0xFFFF
logdev = 0x01
features = ["temp"]
"#;
        let override_family = parse_toml_family(override_toml).unwrap();
        db.merge_family(override_family);

        assert_eq!(db.families.len(), 5);
        let result = db.find_chip(0xAAAA);
        assert!(result.is_some());
    }

    #[test]
    fn test_override_missing_file() {
        let mut db = ChipDatabase::load_compiled();
        let count_before: usize = db.families.iter().map(|f| f.chips.len()).sum();
        db.load_override(Path::new("/nonexistent/path/detect.toml"));
        let count_after: usize = db.families.iter().map(|f| f.chips.len()).sum();
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn test_has_driver() {
        let with_driver = ChipEntry {
            name: "Test".into(),
            driver: "it87".into(),
            devid: 0,
            devid_mask: 0,
            logdev: 0,
            features: vec![],
        };
        assert!(with_driver.has_driver());

        let without_driver = ChipEntry {
            name: "Test".into(),
            driver: String::new(),
            devid: 0,
            devid_mask: 0,
            logdev: 0,
            features: vec![],
        };
        assert!(!without_driver.has_driver());
    }
}
