/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

//! Minimal PCI ID database parser.
//!
//! A focused streaming parser that reads `/usr/share/hwdata/pci.ids` (or alternative paths)
//! line-by-line and extracts vendor, device, subvendor, and subdevice names for a
//! specific set of PCI IDs.

use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Known filesystem locations for the pci.ids database file.
const DB_PATHS: &[&str] = &["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"];

/// Result of looking up a PCI device in the pci.ids database.
/// All four fields are human-readable names from the pci.ids file.
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct PciIdLookup {
    pub vendor_name: Option<String>,
    pub device_name: Option<String>,
    pub subvendor_name: Option<String>,
    pub subdevice_name: Option<String>,
}

/// Opens the first pci.ids file found at one of the known paths.
fn open_db_file() -> Result<File> {
    for path in DB_PATHS {
        if Path::new(path).exists() {
            return File::open(path).map_err(|e| anyhow!("Failed to open {path}: {e}"));
        }
    }
    Err(anyhow!("pci.ids database not found at any known path"))
}

/// Look up PCI device names by vendor/device/subsystem IDs.
///
/// Performs a single streaming pass through the pci.ids file, extracting
/// only the requested names. Stops early once all possible data is found.
pub fn lookup_device(
    vendor_id: u16,
    device_id: u16,
    subsys_vendor_id: u16,
    subsys_device_id: u16,
) -> Result<PciIdLookup> {
    let file = open_db_file()?;
    let reader = BufReader::new(file);
    lookup_device_from_reader(
        reader,
        vendor_id,
        device_id,
        subsys_vendor_id,
        subsys_device_id,
    )
}

/// Tracks parser state during a streaming scan of pci.ids.
struct LookupState {
    result: PciIdLookup,
    vendor_id: u16,
    device_id: u16,
    subsys_vendor_id: u16,
    subsys_device_id: u16,
    in_target_vendor: bool,
    in_target_device: bool,
    /// Whether we've already resolved the subvendor name.
    found_subvendor: bool,
}

/// Returns true when the subvendor is already resolved (or same as vendor).
fn subvendor_resolved(state: &LookupState) -> bool {
    state.found_subvendor || state.subsys_vendor_id == state.vendor_id
}

/// Core lookup logic, separated from I/O for testability.
///
/// Parses the pci.ids line format:
/// - `VVVV  vendor_name`        (vendor: 4 hex digits at column 0)
/// - `\tDDDD  device_name`      (device: tab + 4 hex digits)
/// - `\t\tSSSS SSSS  subsystem` (subdevice: 2 tabs + two 4-hex groups)
/// - `C XX  class_name`         (class section — we stop here)
/// - `#` or empty               (comment/blank — skip)
fn lookup_device_from_reader<R: BufRead>(
    reader: R,
    vendor_id: u16,
    device_id: u16,
    subsys_vendor_id: u16,
    subsys_device_id: u16,
) -> Result<PciIdLookup> {
    let mut st = LookupState {
        result: PciIdLookup::default(),
        vendor_id,
        device_id,
        subsys_vendor_id,
        subsys_device_id,
        in_target_vendor: false,
        in_target_device: false,
        found_subvendor: false,
    };

    for line_result in reader.lines() {
        let line = line_result?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Stop at the class section — no more vendor data after this.
        if line.starts_with("C ") {
            break;
        }
        if process_line(&line, &mut st) {
            break;
        }
    }

    // If subsys vendor == primary vendor, reuse the vendor name.
    if st.result.subvendor_name.is_none() && subsys_vendor_id == vendor_id {
        st.result.subvendor_name = st.result.vendor_name.clone();
    }

    Ok(st.result)
}

/// Dispatches a single non-empty, non-comment line to the correct handler.
/// Returns `true` when the parser should stop (all needed data found).
fn process_line(line: &str, st: &mut LookupState) -> bool {
    debug_assert!(
        !line.is_empty(),
        "Empty lines must be filtered before dispatch."
    );
    let bytes = line.as_bytes();

    if bytes[0] == b'\t' && bytes.get(1) == Some(&b'\t') {
        // Subdevice line: \t\tSVVV SDDD  subsystem_name
        if st.in_target_device {
            if let Some(name) = parse_subdevice_line(line, st.subsys_vendor_id, st.subsys_device_id)
            {
                st.result.subdevice_name = Some(name);
                // Only stop if we already know the subvendor name
                // (or it's the same as the primary vendor). Otherwise,
                // keep scanning to find the subvendor entry later
                // in the file.
                return subvendor_resolved(st);
            }
        }
    } else if bytes[0] == b'\t' {
        // Device line: \tDDDD  device_name
        // Moving to a new device means we won't find a subdevice match
        // for the previous device.
        st.in_target_device = false;
        if st.in_target_vendor {
            if let Some((id, name)) = parse_device_line(line) {
                if id == st.device_id {
                    st.result.device_name = Some(name);
                    st.in_target_device = true;
                }
            }
        }
    } else {
        return process_vendor_line(line, st);
    }
    false
}

/// Handles a vendor-level line (no leading tab). Returns `true` to stop.
fn process_vendor_line(line: &str, st: &mut LookupState) -> bool {
    if st.in_target_vendor {
        // Leaving the target vendor section.
        st.in_target_vendor = false;
        st.in_target_device = false;
        // If subvendor is resolved, we have everything.
        if subvendor_resolved(st) {
            return true;
        }
    }
    if let Some((id, name)) = parse_vendor_line(line) {
        if id == st.vendor_id {
            st.result.vendor_name = Some(name);
            st.in_target_vendor = true;
        } else if id == st.subsys_vendor_id && !st.found_subvendor {
            st.result.subvendor_name = Some(name);
            st.found_subvendor = true;
            // If we already found the primary vendor, we're done.
            if st.result.vendor_name.is_some() {
                return true;
            }
        }
    }
    false
}

/// Parse a vendor line: `VVVV  vendor_name` (4 hex digits at column 0).
fn parse_vendor_line(line: &str) -> Option<(u16, String)> {
    // Minimum: 4 hex + 1 space + 1 char = 6 bytes.
    if line.len() < 6 {
        return None;
    }
    let hex_part = &line[..4];
    // Column 4 must be a space separator per the pci.ids format.
    if line.as_bytes()[4] != b' ' {
        return None;
    }
    let id = u16::from_str_radix(hex_part, 16).ok()?;
    let name = line[4..].trim().to_string();
    debug_assert!(
        !name.is_empty(),
        "Vendor name must not be empty after trim."
    );
    Some((id, name))
}

/// Parse a device line: `\tDDDD  device_name` (tab + 4 hex digits).
fn parse_device_line(line: &str) -> Option<(u16, String)> {
    // Line starts with \t, so real content is at index 1.
    let content = &line[1..];
    if content.len() < 6 {
        return None;
    }
    let hex_part = &content[..4];
    if content.as_bytes()[4] != b' ' {
        return None;
    }
    let id = u16::from_str_radix(hex_part, 16).ok()?;
    let name = content[4..].trim().to_string();
    Some((id, name))
}

/// Parse a subdevice line and check if it matches the target IDs.
/// Format: `\t\tSVVV SDDD  subsystem_name`
fn parse_subdevice_line(
    line: &str,
    target_subvendor: u16,
    target_subdevice: u16,
) -> Option<String> {
    // Line starts with \t\t, so real content is at index 2.
    let content = &line[2..];
    if content.len() < 11 {
        return None;
    }
    let subvendor_hex = &content[..4];
    if content.as_bytes()[4] != b' ' {
        return None;
    }
    let subdevice_hex = &content[5..9];
    let subvendor = u16::from_str_radix(subvendor_hex, 16).ok()?;
    let subdevice = u16::from_str_radix(subdevice_hex, 16).ok()?;
    if subvendor == target_subvendor && subdevice == target_subdevice {
        let name = content[9..].trim().to_string();
        Some(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Minimal pci.ids content for testing. Includes two vendors, one
    /// device with a subdevice entry, and the class-section terminator.
    const TEST_DB: &str = "\
# Test PCI ID database
1234  Test Vendor Inc.
\t5678  Test Device Alpha
\t\t1234 0001  Test Subsystem One
\t\t4321 9999  Other Subsystem
\t5679  Test Device Beta
4321  Other Vendor Corp.
\tab01  Some Other Device
C 00  Unclassified device
";

    fn reader(data: &str) -> Cursor<&[u8]> {
        Cursor::new(data.as_bytes())
    }

    #[test]
    fn lookup_full_match() {
        // Goal: verify that all four name fields are populated when
        // vendor, device, and subdevice all match.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0x1234, 0x5678, 0x1234, 0x0001).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Test Vendor Inc."));
        assert_eq!(result.device_name.as_deref(), Some("Test Device Alpha"));
        assert_eq!(
            result.subvendor_name.as_deref(),
            Some("Test Vendor Inc."),
            "Subvendor same as vendor should reuse vendor name."
        );
        assert_eq!(result.subdevice_name.as_deref(), Some("Test Subsystem One"));
    }

    #[test]
    fn lookup_different_subvendor() {
        // Goal: verify that subvendor name comes from a different vendor
        // entry when subsys_vendor_id differs from vendor_id.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0x1234, 0x5678, 0x4321, 0x9999).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Test Vendor Inc."));
        assert_eq!(result.device_name.as_deref(), Some("Test Device Alpha"));
        assert_eq!(result.subvendor_name.as_deref(), Some("Other Vendor Corp."));
        assert_eq!(result.subdevice_name.as_deref(), Some("Other Subsystem"));
    }

    #[test]
    fn lookup_vendor_only() {
        // Goal: verify that when device ID doesn't match, only vendor
        // name is populated.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0x1234, 0xFFFF, 0x1234, 0x0001).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Test Vendor Inc."));
        assert!(result.device_name.is_none());
        assert!(result.subdevice_name.is_none());
    }

    #[test]
    fn lookup_unknown_vendor() {
        // Goal: verify that a completely unknown vendor returns all None.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0xFFFF, 0x0000, 0xFFFF, 0x0000).unwrap();
        assert!(result.vendor_name.is_none());
        assert!(result.device_name.is_none());
        assert!(result.subvendor_name.is_none());
        assert!(result.subdevice_name.is_none());
    }

    #[test]
    fn lookup_no_subdevice_match() {
        // Goal: verify that when device matches but subdevice doesn't,
        // subdevice_name is None while others are populated.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0x1234, 0x5678, 0x1234, 0xBEEF).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Test Vendor Inc."));
        assert_eq!(result.device_name.as_deref(), Some("Test Device Alpha"));
        assert!(result.subdevice_name.is_none());
    }

    #[test]
    fn lookup_second_device_in_vendor() {
        // Goal: verify that the second device under a vendor is found.
        let result =
            lookup_device_from_reader(reader(TEST_DB), 0x1234, 0x5679, 0x1234, 0x0000).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Test Vendor Inc."));
        assert_eq!(result.device_name.as_deref(), Some("Test Device Beta"));
    }

    #[test]
    fn stops_at_class_section() {
        // Goal: verify the parser stops at the "C " class section marker,
        // not attempting to parse class entries as vendors.
        let db = "\
1234  Vendor
C 00  Some Class
FFFF  Should Not Be Reached
";
        let result = lookup_device_from_reader(reader(db), 0xFFFF, 0x0000, 0xFFFF, 0x0000).unwrap();
        assert!(
            result.vendor_name.is_none(),
            "Parser should stop before reaching FFFF vendor."
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        // Goal: verify comments and blank lines are ignored.
        let db = "\
# This is a comment

1234  Vendor After Blanks
\tabcd  Device
";
        let result = lookup_device_from_reader(reader(db), 0x1234, 0xabcd, 0x1234, 0x0000).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Vendor After Blanks"));
        assert_eq!(result.device_name.as_deref(), Some("Device"));
    }

    #[test]
    fn parse_vendor_line_valid() {
        // Goal: verify vendor line parsing extracts ID and name.
        let (id, name) = parse_vendor_line("8086  Intel Corporation").unwrap();
        assert_eq!(id, 0x8086);
        assert_eq!(name, "Intel Corporation");
    }

    #[test]
    fn parse_vendor_line_too_short() {
        // Goal: verify lines shorter than minimum are rejected.
        assert!(parse_vendor_line("80").is_none());
    }

    #[test]
    fn parse_vendor_line_invalid_hex() {
        // Goal: verify non-hex vendor IDs are rejected.
        assert!(parse_vendor_line("ZZZZ  Bad Vendor").is_none());
    }

    #[test]
    fn parse_device_line_valid() {
        // Goal: verify device line parsing (with leading tab stripped).
        let (id, name) = parse_device_line("\t1234  Some Device").unwrap();
        assert_eq!(id, 0x1234);
        assert_eq!(name, "Some Device");
    }

    #[test]
    fn parse_subdevice_line_match() {
        // Goal: verify subdevice line matches target IDs.
        let name = parse_subdevice_line("\t\t1234 5678  My Subsystem", 0x1234, 0x5678);
        assert_eq!(name.as_deref(), Some("My Subsystem"));
    }

    #[test]
    fn parse_subdevice_line_no_match() {
        // Goal: verify subdevice line returns None for non-matching IDs.
        let name = parse_subdevice_line("\t\t1234 5678  My Subsystem", 0x0000, 0x0000);
        assert!(name.is_none());
    }

    #[test]
    fn subvendor_before_vendor_in_file() {
        // Goal: verify that when the subsys vendor appears BEFORE the
        // primary vendor in the file, both are still found.
        let db = "\
4321  Subvendor First
1234  Primary Vendor
\t5678  Device
\t\t4321 0001  Subsystem
";
        let result = lookup_device_from_reader(reader(db), 0x1234, 0x5678, 0x4321, 0x0001).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Primary Vendor"));
        assert_eq!(result.subvendor_name.as_deref(), Some("Subvendor First"));
        assert_eq!(result.subdevice_name.as_deref(), Some("Subsystem"));
    }

    #[test]
    fn empty_database() {
        // Goal: verify empty input returns all None without error.
        let result = lookup_device_from_reader(reader(""), 0x1234, 0x5678, 0x1234, 0x0001).unwrap();
        assert!(result.vendor_name.is_none());
        assert!(result.device_name.is_none());
    }

    #[test]
    fn subvendor_after_vendor_no_subdevice() {
        // Goal: verify when subvendor appears after the primary vendor and
        // there is no subdevice match, both vendor names are still found.
        let db = "\
1234  Primary Vendor
\t5678  Device
\t\tFFFF FFFF  Wrong Subsystem
4321  Subvendor After
";
        let result = lookup_device_from_reader(reader(db), 0x1234, 0x5678, 0x4321, 0x0001).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Primary Vendor"));
        assert_eq!(result.device_name.as_deref(), Some("Device"));
        assert_eq!(
            result.subvendor_name.as_deref(),
            Some("Subvendor After"),
            "Subvendor that appears after the primary vendor section."
        );
        assert!(
            result.subdevice_name.is_none(),
            "No matching subdevice should yield None."
        );
    }

    #[test]
    fn subdevice_found_subvendor_later() {
        // Goal: the critical case — subdevice match is found inside the
        // primary vendor section, but the subvendor name appears later.
        // Parser must NOT stop at the subdevice match; it must continue
        // scanning to resolve the subvendor name.
        let db = "\
aaaa  Primary
\tbbbb  Device X
\t\tcccc dddd  Matched Subsystem
cccc  The Subvendor
";
        let result = lookup_device_from_reader(reader(db), 0xaaaa, 0xbbbb, 0xcccc, 0xdddd).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Primary"));
        assert_eq!(result.device_name.as_deref(), Some("Device X"));
        assert_eq!(
            result.subdevice_name.as_deref(),
            Some("Matched Subsystem"),
            "Subdevice found inside the primary vendor section."
        );
        assert_eq!(
            result.subvendor_name.as_deref(),
            Some("The Subvendor"),
            "Subvendor appearing after subdevice must be resolved."
        );
    }

    #[test]
    fn subvendor_already_seen_before_subdevice() {
        // Goal: when subvendor appears before the primary vendor,
        // subdevice match should cause immediate stop since
        // subvendor is already resolved.
        let db = "\
cccc  Known Subvendor
aaaa  Primary
\tbbbb  Device X
\t\tcccc dddd  Matched Subsystem
ffff  Should Not Be Reached
";
        let result = lookup_device_from_reader(reader(db), 0xaaaa, 0xbbbb, 0xcccc, 0xdddd).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Primary"));
        assert_eq!(result.subvendor_name.as_deref(), Some("Known Subvendor"));
        assert_eq!(result.subdevice_name.as_deref(), Some("Matched Subsystem"));
    }

    #[test]
    fn only_comments_database() {
        // Goal: verify a database with only comments returns all None.
        let db = "\
# Comment 1
# Comment 2
# Comment 3
";
        let result = lookup_device_from_reader(reader(db), 0x1234, 0x5678, 0x1234, 0x0001).unwrap();
        assert!(result.vendor_name.is_none());
    }

    #[test]
    fn vendor_with_no_devices() {
        // Goal: verify a vendor entry with no device children
        // still resolves the vendor name.
        let db = "\
1234  Lone Vendor
5678  Another Vendor
";
        let result = lookup_device_from_reader(reader(db), 0x1234, 0x0001, 0x1234, 0x0000).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Lone Vendor"));
        assert!(result.device_name.is_none());
    }

    #[test]
    fn parse_device_line_too_short() {
        // Goal: verify device lines shorter than minimum are rejected.
        assert!(parse_device_line("\t80").is_none());
    }

    #[test]
    fn parse_device_line_no_space_separator() {
        // Goal: verify device lines without the required space are rejected.
        assert!(parse_device_line("\t1234X Device").is_none());
    }

    #[test]
    fn parse_subdevice_line_too_short() {
        // Goal: verify subdevice lines shorter than minimum are rejected.
        assert!(parse_subdevice_line("\t\t1234 567", 0x1234, 0x567).is_none());
    }

    #[test]
    fn parse_subdevice_line_no_space_between_ids() {
        // Goal: verify subdevice lines without space between IDs are rejected.
        assert!(parse_subdevice_line("\t\t1234X5678  Name", 0x1234, 0x5678).is_none());
    }

    #[test]
    fn parse_vendor_line_no_space_separator() {
        // Goal: verify vendor lines without the required space are rejected.
        assert!(parse_vendor_line("1234XBad Vendor").is_none());
    }

    #[test]
    fn boundary_ids_zero() {
        // Goal: verify zero IDs (0x0000) work correctly.
        let db = "\
0000  Zero Vendor
\t0000  Zero Device
\t\t0000 0000  Zero Subsystem
";
        let result = lookup_device_from_reader(reader(db), 0x0000, 0x0000, 0x0000, 0x0000).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Zero Vendor"));
        assert_eq!(result.device_name.as_deref(), Some("Zero Device"));
        assert_eq!(result.subdevice_name.as_deref(), Some("Zero Subsystem"));
    }

    #[test]
    fn boundary_ids_max() {
        // Goal: verify maximum IDs (0xFFFF) work correctly.
        let db = "\
ffff  Max Vendor
\tffff  Max Device
\t\tffff ffff  Max Subsystem
";
        let result = lookup_device_from_reader(reader(db), 0xffff, 0xffff, 0xffff, 0xffff).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Max Vendor"));
        assert_eq!(result.device_name.as_deref(), Some("Max Device"));
        assert_eq!(result.subdevice_name.as_deref(), Some("Max Subsystem"));
    }

    #[test]
    fn stops_scanning_for_subvendor_at_class_section() {
        // Goal: verify parser stops at class section even when still
        // looking for a subvendor.
        let db = "\
aaaa  Primary
\tbbbb  Device
\t\tcccc dddd  Subsystem
C 00  Unclassified
cccc  Unreachable Subvendor
";
        let result = lookup_device_from_reader(reader(db), 0xaaaa, 0xbbbb, 0xcccc, 0xdddd).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Primary"));
        assert_eq!(result.subdevice_name.as_deref(), Some("Subsystem"));
        assert!(
            result.subvendor_name.is_none(),
            "Subvendor after class section should not be reached."
        );
    }

    #[test]
    fn multiple_vendors_same_device_id() {
        // Goal: verify the correct device is matched under the right vendor,
        // not under a different vendor with the same device ID.
        let db = "\
aaaa  Vendor A
\t1111  Device From A
bbbb  Vendor B
\t1111  Device From B
";
        let result = lookup_device_from_reader(reader(db), 0xbbbb, 0x1111, 0xbbbb, 0x0000).unwrap();
        assert_eq!(result.vendor_name.as_deref(), Some("Vendor B"));
        assert_eq!(
            result.device_name.as_deref(),
            Some("Device From B"),
            "Device should come from Vendor B, not Vendor A."
        );
    }
}
