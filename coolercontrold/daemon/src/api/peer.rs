// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Peer identity for per-client limits.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Bytes of an IPv6 address that name its network: the /64 an ISP or LAN hands one host.
const IPV6_NETWORK_BYTES: usize = 8;

const _: () = assert!(IPV6_NETWORK_BYTES * 8 == u64::BITS as usize);

/// The unit a per-client budget is charged to.
///
/// A single address is the wrong unit twice over. Every address in 127/8, and `::1`, reach
/// this same host, and any local process can bind one as its source, so a key per address
/// would hand a local user unlimited fresh budgets. An IPv6 host is normally given a whole
/// /64, so a key per address would hand it 2^64 of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerKey {
    Loopback,
    V4(Ipv4Addr),
    /// The upper 64 bits of an IPv6 address.
    V6Net(u64),
}

impl PeerKey {
    pub fn from_ip(address: IpAddr) -> Self {
        if is_loopback(address) {
            return Self::Loopback;
        }
        match address.to_canonical() {
            IpAddr::V4(v4) => Self::V4(v4),
            IpAddr::V6(v6) => Self::V6Net(ipv6_network(v6)),
        }
    }
}

/// Whether `address` is this host. Unlike `IpAddr::is_loopback`, this also recognises the
/// IPv4-mapped form `::ffff:127.0.0.1` that a dual-stack `::` listener reports for local
/// IPv4 clients.
pub fn is_loopback(address: IpAddr) -> bool {
    address.to_canonical().is_loopback()
}

fn ipv6_network(address: Ipv6Addr) -> u64 {
    let octets = address.octets();
    let mut network = [0_u8; IPV6_NETWORK_BYTES];
    network.copy_from_slice(&octets[..IPV6_NETWORK_BYTES]);
    u64::from_be_bytes(network)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    fn key(address: &str) -> PeerKey {
        PeerKey::from_ip(address.parse().unwrap())
    }

    /// Goal: an IPv4 client reached through a dual-stack listener is the same peer as when it
    /// connects over IPv4, so it cannot hold two budgets.
    #[test]
    fn mapped_ipv4_is_the_ipv4_peer() {
        assert_eq!(key("::ffff:192.0.2.7"), key("192.0.2.7"));
        assert_eq!(key("192.0.2.7"), PeerKey::V4(Ipv4Addr::new(192, 0, 2, 7)));
    }

    /// Goal: every spelling of this host collapses to one key, so rotating source addresses
    /// through 127/8 buys nothing.
    #[test]
    fn every_loopback_address_is_one_peer() {
        for address in [
            "127.0.0.1",
            "127.0.0.2",
            "127.255.255.254",
            "::1",
            "::ffff:127.0.0.9",
        ] {
            assert_eq!(key(address), PeerKey::Loopback, "{address}");
        }
    }

    /// Goal: loopback is decided before the /64 mask. Masked, `::1` would read as the `::`
    /// network and lose its loopback status.
    #[test]
    fn ipv6_loopback_is_not_the_unspecified_network() {
        assert_ne!(key("::1"), key("::2"));
        assert_eq!(key("::2"), PeerKey::V6Net(0));
    }

    /// Goal: addresses inside one /64 share a key, and neighbouring /64s do not.
    #[test]
    fn ipv6_peers_are_keyed_by_their_64() {
        assert_eq!(
            key("2001:db8:1:2::1"),
            key("2001:db8:1:2:ffff:ffff:ffff:ffff")
        );
        assert_ne!(key("2001:db8:1:2::1"), key("2001:db8:1:3::1"));
        assert_eq!(
            key("2001:db8:1:2::1"),
            PeerKey::V6Net(0x2001_0db8_0001_0002)
        );
    }

    /// Goal: distinct IPv4 addresses stay distinct, and private ones are not loopback.
    #[test]
    fn ipv4_peers_are_keyed_by_address() {
        assert_ne!(key("192.0.2.1"), key("192.0.2.2"));
        assert_ne!(key("192.0.2.1"), PeerKey::Loopback);
        assert_ne!(key("10.0.0.1"), PeerKey::Loopback);
    }

    /// Goal: the standalone check agrees with the key for both loopback forms and a remote.
    #[test]
    fn is_loopback_recognises_mapped_addresses() {
        assert!(is_loopback("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_loopback("::1".parse().unwrap()));
        assert!(is_loopback("::ffff:192.0.2.1".parse().unwrap()).not());
    }
}
