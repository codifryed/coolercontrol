// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Encode a byte slice as a lowercase hex string.
pub fn to_lower_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, byte| {
            // `fmt::Write` for `String` is infallible; it only calls `push_str` internally.
            write!(acc, "{byte:02x}").unwrap();
            acc
        })
}

/// SHA-256 of `bytes` as colon-separated lowercase byte pairs.
///
/// The grouping matches `coolercontrol/tls_trust.cpp::fingerprint`, so a fingerprint the
/// daemon prints can be compared against one the desktop app shows without transcribing.
/// Shared by the TLS server (which reports its own certificate) and the plugin client
/// (which pins a remote's), so the two can never disagree on the format.
pub fn to_fingerprint(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex = to_lower_hex(&digest);
    let mut grouped = String::with_capacity(hex.len() + hex.len() / 2);
    for (index, pair) in hex.as_bytes().chunks(2).enumerate() {
        if index > 0 {
            grouped.push(':');
        }
        grouped.push_str(std::str::from_utf8(pair).unwrap_or_default());
    }
    grouped
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

    /// Goal: the fingerprint format is a contract with two other places, the desktop
    /// app's display and the `tls_pin` file a user edits by hand. Drifting from it would
    /// silently stop pins from matching.
    #[test]
    fn test_to_fingerprint_format() {
        // SHA-256 of the empty input, the standard vector.
        let printed = to_fingerprint(&[]);
        assert!(printed.starts_with("e3:b0:c4:42:98:fc:1c:14"));
        assert_eq!(printed.split(':').count(), 32);
        assert_eq!(printed.len(), 32 * 3 - 1);
        for group in printed.split(':') {
            assert_eq!(group.len(), 2);
            assert!(group.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_to_fingerprint_is_stable_and_distinct() {
        assert_eq!(to_fingerprint(b"abc"), to_fingerprint(b"abc"));
        assert_ne!(to_fingerprint(b"abc"), to_fingerprint(b"abd"));
    }
}
