/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fmt::Write;

/// Encode a byte slice as a lowercase hex string.
pub fn to_lower_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut acc, byte| {
            write!(acc, "{byte:02x}").unwrap();
            acc
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_lower_hex_empty() {
        assert_eq!(to_lower_hex(&[]), "");
    }

    #[test]
    fn test_to_lower_hex_single_byte() {
        assert_eq!(to_lower_hex(&[0xff]), "ff");
        assert_eq!(to_lower_hex(&[0x00]), "00");
        assert_eq!(to_lower_hex(&[0x0a]), "0a");
    }

    #[test]
    fn test_to_lower_hex_multiple_bytes() {
        assert_eq!(to_lower_hex(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }
}
