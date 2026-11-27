// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Names for JEDEC JEP106 manufacturer IDs, which memory modules carry in place of a name.
//!
//! A short list of memory makers, not the whole registry: most of its 1500 or so entries
//! never made a module, no distribution ships it as a file the way `pci.ids` is, and its
//! names read poorly ("G Skill Intl"). The names here are the brands as users know them. The
//! IDs were checked against the table in i2c-tools' `decode-dimms`.

/// Clears the odd-parity bit both ID bytes carry in their top bit.
const PARITY_MASK: u8 = 0x7f;

/// `(bank, code, name)`. The bank is 1-based, as JEP106 numbers them.
const MAKERS: [(u8, u8, &str); 32] = [
    (1, 0x2c, "Micron"),
    (1, 0x2d, "SK hynix"),
    (1, 0x4e, "Samsung"),
    (2, 0x18, "Kingston"),
    (2, 0x32, "Mushkin"),
    (2, 0x3a, "PNY"),
    (2, 0x4f, "Transcend"),
    (2, 0x7a, "Apacer"),
    (3, 0x1e, "Corsair"),
    (4, 0x0b, "Nanya"),
    (4, 0x13, "GeIL"),
    (4, 0x14, "Mushkin"),
    (4, 0x25, "Kingmax"),
    (5, 0x43, "Ramaxel"),
    (5, 0x4b, "ADATA"),
    (5, 0x4d, "G.Skill"),
    (5, 0x6f, "TeamGroup"),
    (6, 0x02, "Patriot"),
    (6, 0x1b, "Crucial"),
    (7, 0x53, "Silicon Power"),
    (7, 0x5e, "Lexar"),
    (7, 0x6d, "V-Color"),
    (8, 0x46, "Gloway"),
    (9, 0x13, "Gloway"),
    (9, 0x18, "Essencore"),
    (9, 0x71, "Asgard"),
    (10, 0x68, "Kimtigo"),
    (10, 0x6c, "Colorful"),
    (10, 0x77, "Netac"),
    (11, 0x11, "CXMT"),
    (11, 0x42, "Thermaltake"),
    (11, 0x76, "Lexar"),
];

/// The maker behind a two-byte manufacturer ID as an SPD stores it: the count of continuation
/// codes, which is the bank less one, then the code within that bank. None for a maker that
/// is not listed.
pub fn maker_name(continuation_count: u8, code: u8) -> Option<&'static str> {
    let bank = (continuation_count & PARITY_MASK) + 1;
    let code = code & PARITY_MASK;
    MAKERS
        .iter()
        .find(|(maker_bank, maker_code, _)| *maker_bank == bank && *maker_code == code)
        .map(|(_, _, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    /// Goal: an ID resolves as an SPD stores it, parity bits included.
    /// Method: the two IDs read from a real G.Skill module with SK hynix chips.
    #[test]
    fn resolves_ids_as_read_from_a_module() {
        assert_eq!(maker_name(0x04, 0xcd), Some("G.Skill"));
        assert_eq!(maker_name(0x80, 0xad), Some("SK hynix"));
    }

    /// Goal: the parity bit does not decide the match, as some modules leave it unset.
    /// Method: the same ID with each byte's top bit flipped.
    #[test]
    fn ignores_the_parity_bits() {
        assert_eq!(maker_name(0x84, 0x4d), Some("G.Skill"));
        assert_eq!(maker_name(0x00, 0x2d), Some("SK hynix"));
    }

    /// Goal: an unlisted or blank ID gives no name, never a neighbour's.
    /// Method: blank IDs, a listed code in the wrong bank, and the highest bank value.
    #[test]
    fn gives_none_for_an_unlisted_id() {
        assert_eq!(maker_name(0x00, 0x00), None);
        assert_eq!(maker_name(0xff, 0xff), None);
        // G.Skill's code, one bank early.
        assert_eq!(maker_name(0x03, 0xcd), None);
    }

    /// Goal: the table holds no ID twice, so a lookup cannot depend on its order.
    /// Method: compare every pair of entries.
    #[test]
    fn lists_no_id_twice() {
        for (index, (bank, code, _)) in MAKERS.iter().enumerate() {
            let repeated = MAKERS[index + 1..]
                .iter()
                .any(|(other_bank, other_code, _)| other_bank == bank && other_code == code);
            assert!(repeated.not(), "bank {bank} code {code:#04x}");
            assert!(*bank >= 1);
            assert!(*code <= PARITY_MASK);
        }
    }
}
