// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decodes what identifies a DDR5 memory module from its SPD EEPROM contents, as laid out by
//! JEDEC JESD400-5. Decoding only: the caller reads the bytes.

use crate::repositories::hwmon::jedec;
use std::fmt::Write;
use std::ops::{Not, Range};

const MEMORY_TYPE_OFFSET: usize = 2;
const MEMORY_TYPE_DDR5: u8 = 0x12;
const MEMORY_TYPE_NAME: &str = "DDR5";
/// SDRAM density per die in bits 4:0, dies per package in bits 7:5.
const DENSITY_PACKAGE_OFFSET: usize = 4;
/// SDRAM I/O width in bits 7:5.
const IO_WIDTH_OFFSET: usize = 6;
/// Package ranks less one in bits 5:3, and bit 6 set for a module mixing two kinds of SDRAM.
const ORGANIZATION_OFFSET: usize = 234;
/// Primary bus width per sub-channel in bits 2:0, sub-channels in bits 7:5.
const BUS_WIDTH_OFFSET: usize = 235;
const MODULE_MAKER_ID: Range<usize> = 512..514;
/// ASCII, padded with spaces.
const PART_NUMBER: Range<usize> = 521..551;
/// Fits a maker, a capacity and a full 30 character part number without growing.
const MODEL_CAPACITY: usize = 64;

const _: () = assert!(MEMORY_TYPE_OFFSET < DENSITY_PACKAGE_OFFSET);
const _: () = assert!(BUS_WIDTH_OFFSET < MODULE_MAKER_ID.start);
const _: () = assert!(MODULE_MAKER_ID.end <= PART_NUMBER.start);
const _: () = assert!(PART_NUMBER.end - PART_NUMBER.start < MODEL_CAPACITY);

/// A model name for the module, such as `G.Skill 24 GB DDR5 (F5-8400J4052G24G)`.
///
/// Each part is left out when the contents do not give it. None for any other memory type,
/// whose layout differs, and when the memory type is all there is to say.
pub fn model(eeprom: &[u8]) -> Option<String> {
    if eeprom.get(MEMORY_TYPE_OFFSET) != Some(&MEMORY_TYPE_DDR5) {
        return None;
    }
    let maker = maker(eeprom);
    let capacity_gb = capacity_gb(eeprom);
    let part_number = part_number(eeprom);
    let known = [
        maker.is_some(),
        capacity_gb.is_some(),
        part_number.is_some(),
    ];
    if known.contains(&true).not() {
        return None;
    }
    let mut model = String::with_capacity(MODEL_CAPACITY);
    if let Some(maker) = maker {
        model.push_str(maker);
        model.push(' ');
    }
    if let Some(capacity_gb) = capacity_gb {
        // Writing to a String cannot fail.
        let _ = write!(model, "{capacity_gb} GB ");
    }
    model.push_str(MEMORY_TYPE_NAME);
    if let Some(part_number) = part_number {
        model.push_str(" (");
        model.push_str(&part_number);
        model.push(')');
    }
    debug_assert!(model.is_empty().not());
    Some(model)
}

fn maker(eeprom: &[u8]) -> Option<&'static str> {
    let id = eeprom.get(MODULE_MAKER_ID)?;
    debug_assert_eq!(id.len(), 2);
    jedec::maker_name(id[0], id[1])
}

/// The module's capacity in GB. None when a field holds a value the standard does not
/// define, and for a module mixing two kinds of SDRAM, which further bytes describe.
///
/// Per sub-channel, `bus width / I/O width` packages hold `dies * density` gigabits each:
///
/// ```text
/// gigabits = sub_channels * (bus_width / io_width) * dies * density * ranks
/// ```
///
/// A 24 GB module: `2 * (32 / 8) * 1 * 24 * 1 = 192` gigabits, and `192 / 8 = 24` GB.
fn capacity_gb(eeprom: &[u8]) -> Option<u32> {
    let density_package = *eeprom.get(DENSITY_PACKAGE_OFFSET)?;
    let io_width = *eeprom.get(IO_WIDTH_OFFSET)?;
    let organization = *eeprom.get(ORGANIZATION_OFFSET)?;
    let bus_width = *eeprom.get(BUS_WIDTH_OFFSET)?;
    let is_asymmetric = organization & 0x40 != 0;
    if is_asymmetric {
        return None;
    }
    let density_gbit: u32 = match density_package & 0x1f {
        1 => 4,
        2 => 8,
        3 => 12,
        4 => 16,
        5 => 24,
        6 => 32,
        7 => 48,
        8 => 64,
        _ => return None,
    };
    // One die, two dies, then 3DS stacks of 2, 4, 8 and 16.
    let dies: u32 = match density_package >> 5 {
        0 => 1,
        1 | 2 => 2,
        3 => 4,
        4 => 8,
        5 => 16,
        _ => return None,
    };
    let io_width_bits: u32 = match io_width >> 5 {
        0 => 4,
        1 => 8,
        2 => 16,
        3 => 32,
        _ => return None,
    };
    let bus_width_bits: u32 = match bus_width & 0x07 {
        0 => 8,
        1 => 16,
        2 => 32,
        3 => 64,
        _ => return None,
    };
    // Higher values exist, but how they count is not confirmed.
    let sub_channels: u32 = match bus_width >> 5 {
        0 => 1,
        1 => 2,
        _ => return None,
    };
    let ranks = u32::from((organization >> 3) & 0x07) + 1;
    if bus_width_bits.is_multiple_of(io_width_bits).not() {
        return None;
    }
    let packages = bus_width_bits / io_width_bits;
    // At most 2 * 16 * 16 * 64 * 8, far inside a u32.
    let gigabits = sub_channels * packages * dies * density_gbit * ranks;
    if gigabits.is_multiple_of(8).not() {
        return None;
    }
    Some(gigabits / 8)
}

/// The part number with its padding removed. None for a field that is blank or not printable
/// ASCII.
fn part_number(eeprom: &[u8]) -> Option<String> {
    let field = eeprom.get(PART_NUMBER)?;
    debug_assert_eq!(field.len(), PART_NUMBER.len());
    // Some modules pad with NULs where the standard asks for spaces.
    let is_padding = |byte: &u8| *byte == b' ' || *byte == 0;
    let start = field.iter().position(|byte| is_padding(byte).not())?;
    let end = field.iter().rposition(|byte| is_padding(byte).not())? + 1;
    let text = &field[start..end];
    if text
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
        .not()
    {
        return None;
    }
    String::from_utf8(text.to_vec()).ok()
}

/// SPD contents for tests, built field by field.
#[cfg(test)]
pub struct TestSpd(Vec<u8>);

#[cfg(test)]
impl TestSpd {
    /// Blank DDR5 contents of `len` bytes.
    pub fn ddr5(len: usize) -> Self {
        let mut eeprom = vec![0u8; len];
        if let Some(byte) = eeprom.get_mut(MEMORY_TYPE_OFFSET) {
            *byte = MEMORY_TYPE_DDR5;
        }
        Self(eeprom)
    }

    /// The fields of a real 24 GB G.Skill module, as read from one.
    pub fn g_skill_24_gb() -> Self {
        Self::ddr5(1024)
            .geometry(0x05, 0x20, 0x00, 0x22)
            .maker(0x04, 0xcd)
            .part_number(b"F5-8400J4052G24G              ")
    }

    pub fn geometry(
        mut self,
        density_package: u8,
        io_width: u8,
        organization: u8,
        bus: u8,
    ) -> Self {
        self.0[DENSITY_PACKAGE_OFFSET] = density_package;
        self.0[IO_WIDTH_OFFSET] = io_width;
        self.0[ORGANIZATION_OFFSET] = organization;
        self.0[BUS_WIDTH_OFFSET] = bus;
        self
    }

    pub fn maker(mut self, continuation_count: u8, code: u8) -> Self {
        self.0[MODULE_MAKER_ID.start] = continuation_count;
        self.0[MODULE_MAKER_ID.start + 1] = code;
        self
    }

    pub fn part_number(mut self, part_number: &[u8]) -> Self {
        let field = &mut self.0[PART_NUMBER];
        field.fill(0);
        field[..part_number.len()].copy_from_slice(part_number);
        self
    }

    pub fn memory_type(mut self, memory_type: u8) -> Self {
        self.0[MEMORY_TYPE_OFFSET] = memory_type;
        self
    }

    pub fn bytes(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a real module decodes to its full model name.
    /// Method: the fields read from a 24 GB G.Skill module.
    #[test]
    fn names_a_real_module_in_full() {
        let eeprom = TestSpd::g_skill_24_gb().bytes();
        assert_eq!(
            model(&eeprom).as_deref(),
            Some("G.Skill 24 GB DDR5 (F5-8400J4052G24G)")
        );
    }

    /// Goal: each part the contents do not give is left out, and the rest still shows.
    /// Method: drop the maker, the geometry and the part number in turn.
    #[test]
    fn leaves_out_what_the_contents_do_not_give() {
        let unlisted_maker = TestSpd::g_skill_24_gb().maker(0x00, 0x00).bytes();
        assert_eq!(
            model(&unlisted_maker).as_deref(),
            Some("24 GB DDR5 (F5-8400J4052G24G)")
        );
        let no_geometry = TestSpd::g_skill_24_gb().geometry(0, 0, 0, 0).bytes();
        assert_eq!(
            model(&no_geometry).as_deref(),
            Some("G.Skill DDR5 (F5-8400J4052G24G)")
        );
        let no_part_number = TestSpd::g_skill_24_gb().part_number(b"").bytes();
        assert_eq!(
            model(&no_part_number).as_deref(),
            Some("G.Skill 24 GB DDR5")
        );
    }

    /// Goal: contents that identify nothing give no model, never the memory type alone.
    /// Method: blank DDR5 contents, another memory type, a first page only, and nothing.
    #[test]
    fn gives_none_when_nothing_identifies_the_module() {
        assert_eq!(model(&TestSpd::ddr5(1024).bytes()), None);
        let ddr4 = TestSpd::g_skill_24_gb().memory_type(0x0c).bytes();
        assert_eq!(model(&ddr4), None);
        // Only the first page, as a write-protected hub gives.
        assert_eq!(model(&TestSpd::ddr5(128).bytes()), None);
        assert_eq!(model(&[]), None);
    }

    /// Goal: capacity follows the density, width, die count and rank fields.
    /// Method: known module layouts, each with its gigabyte total.
    #[test]
    fn computes_capacity_from_the_geometry() {
        let capacity = |density_package, io_width, organization, bus| {
            let eeprom = TestSpd::ddr5(1024)
                .geometry(density_package, io_width, organization, bus)
                .bytes();
            capacity_gb(&eeprom)
        };
        // 1 rank of 16 Gb x8 dies on two 32-bit sub-channels.
        assert_eq!(capacity(0x04, 0x20, 0x00, 0x22), Some(16));
        // The same with 2 ranks.
        assert_eq!(capacity(0x04, 0x20, 0x08, 0x22), Some(32));
        // 1 rank of 24 Gb x8 dies.
        assert_eq!(capacity(0x05, 0x20, 0x00, 0x22), Some(24));
        // 2 ranks of 16 Gb x4 dies, as on a 64 GB registered module.
        assert_eq!(capacity(0x04, 0x00, 0x08, 0x22), Some(64));
        // 1 rank of 16 Gb x16 dies, as on an 8 GB module.
        assert_eq!(capacity(0x04, 0x40, 0x00, 0x22), Some(8));
        // 2 ranks of 16 Gb x4 dies in 3DS stacks of 4.
        assert_eq!(capacity(0x64, 0x00, 0x08, 0x22), Some(256));
    }

    /// Goal: a field the standard does not define, or a layout this does not cover, gives no
    /// capacity instead of a wrong one.
    /// Method: one out-of-range field at a time, then an asymmetric module.
    #[test]
    fn gives_no_capacity_for_undefined_fields() {
        let capacity = |density_package, io_width, organization, bus| {
            let eeprom = TestSpd::ddr5(1024)
                .geometry(density_package, io_width, organization, bus)
                .bytes();
            capacity_gb(&eeprom)
        };
        assert_eq!(capacity(0x00, 0x20, 0x00, 0x22), None, "no density");
        assert_eq!(capacity(0x09, 0x20, 0x00, 0x22), None, "density past 64 Gb");
        assert_eq!(capacity(0xc4, 0x20, 0x00, 0x22), None, "die count");
        assert_eq!(capacity(0x04, 0x80, 0x00, 0x22), None, "I/O width");
        assert_eq!(capacity(0x04, 0x20, 0x00, 0x24), None, "bus width");
        assert_eq!(capacity(0x04, 0x20, 0x00, 0x42), None, "sub-channels");
        assert_eq!(capacity(0x04, 0x20, 0x40, 0x22), None, "asymmetric");
        // An 8-bit bus cannot be built from 16-bit packages.
        assert_eq!(capacity(0x04, 0x40, 0x00, 0x20), None, "width mismatch");
        assert_eq!(capacity_gb(&TestSpd::ddr5(128).bytes()), None, "short read");
    }

    /// Goal: the part number is taken without its padding, whether spaces or NULs.
    /// Method: a real part number padded each way, and one with a space inside.
    #[test]
    fn part_number_is_read_without_its_padding() {
        let spaces = TestSpd::ddr5(1024)
            .part_number(b"F5-8400J4052G24G              ")
            .bytes();
        assert_eq!(part_number(&spaces).as_deref(), Some("F5-8400J4052G24G"));
        let nuls = TestSpd::ddr5(1024).part_number(b"F5-8400J4052G24G").bytes();
        assert_eq!(part_number(&nuls).as_deref(), Some("F5-8400J4052G24G"));
        let inner_space = TestSpd::ddr5(1024).part_number(b"  CMK32 GX5  ").bytes();
        assert_eq!(part_number(&inner_space).as_deref(), Some("CMK32 GX5"));
    }

    /// Goal: a blank or unprintable part number gives none, never garbage.
    /// Method: spaces only, NULs only, bytes outside ASCII, and a short read.
    #[test]
    fn part_number_is_none_when_blank_or_unprintable() {
        let blank = TestSpd::ddr5(1024)
            .part_number(b"                              ")
            .bytes();
        assert_eq!(part_number(&blank), None);
        assert_eq!(part_number(&TestSpd::ddr5(1024).bytes()), None);
        let unprintable = TestSpd::ddr5(1024).part_number(b"F5-\xff\x01").bytes();
        assert_eq!(part_number(&unprintable), None);
        assert_eq!(part_number(&TestSpd::ddr5(128).bytes()), None);
    }
}
