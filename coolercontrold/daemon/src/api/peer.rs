// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Peer identity for per-client limits, and the client behind a trusted reverse proxy.

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use log::error;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::Not;
use std::str::FromStr;
use std::sync::Arc;

/// Bytes of an IPv6 address that name its network: the /64 an ISP or LAN hands one host.
const IPV6_NETWORK_BYTES: usize = 8;

const _: () = assert!(IPV6_NETWORK_BYTES * 8 == u64::BITS as usize);

/// The most reverse proxies a config may name. A handful is typical.
const MAX_TRUSTED_PROXIES: usize = 64;
/// The most `X-Forwarded-For` hops walked back from the nearest. Past that many trusted
/// hops the chain is not a real deployment, and the walk stops at the last one reached.
const MAX_FORWARDED_HOPS: usize = 16;
const X_FORWARDED_FOR: &str = "x-forwarded-for";

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

/// The client a request came from: its TCP peer, or behind a trusted reverse proxy, the
/// address the proxy forwarded. Inserted for every request by `client_addr_middleware`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddr(pub IpAddr);

/// Resolves `ClientAddr` before any per-client limit reads it.
///
/// Only the auth budgets use it. The HTTPS redirect and the connection limits stay on the
/// TCP peer, because they are properties of the connection itself.
pub async fn client_addr_middleware(
    State(trusted): State<Arc<TrustedProxies>>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    if let Some(peer) = peer {
        let client = resolve_client(peer, request.headers(), &trusted);
        request.extensions_mut().insert(ClientAddr(client));
    }
    next.run(request).await
}

/// The client behind `peer`. `X-Forwarded-For` is read only when `peer` is a trusted proxy,
/// since anyone else can write whatever it likes there.
///
/// The list is walked from the right, the hop nearest this daemon, skipping hops that are
/// themselves trusted proxies. The first untrusted address is the client. A client can
/// prepend anything it likes, but it cannot get past the address its own proxy appended.
/// A hop that does not parse ends the walk at the last trusted hop reached, which groups
/// such requests under that proxy: the safe direction to fail.
pub fn resolve_client(peer: IpAddr, headers: &HeaderMap, trusted: &TrustedProxies) -> IpAddr {
    if trusted.contains(peer).not() {
        return peer;
    }
    let hops = headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .rev()
        .take(MAX_FORWARDED_HOPS);
    let mut nearest_trusted = peer;
    for hop in hops {
        debug_assert!(
            trusted.contains(nearest_trusted),
            "the walk only passes trusted hops"
        );
        let Some(address) = parse_hop(hop) else {
            return nearest_trusted;
        };
        if trusted.contains(address).not() {
            return address;
        }
        nearest_trusted = address;
    }
    nearest_trusted
}

/// One `X-Forwarded-For` entry: a bare address, or one with a port as some proxies write it.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    debug_assert!(hop.contains(',').not(), "the caller splits the list");
    let hop = hop.trim();
    if let Ok(address) = hop.parse::<IpAddr>() {
        return Some(address);
    }
    if let Ok(address) = hop.parse::<SocketAddr>() {
        return Some(address.ip());
    }
    hop.strip_prefix('[')?.strip_suffix(']')?.parse().ok()
}

/// The configured reverse proxies whose `X-Forwarded-For` is believed.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies {
    networks: Box<[IpNet]>,
}

impl TrustedProxies {
    /// Parses the configured entries. An invalid entry is logged and skipped: that proxy
    /// then stays untrusted, which is the safe direction to fail.
    pub fn from_config(entries: &[String]) -> Self {
        let mut networks = Vec::with_capacity(entries.len().min(MAX_TRUSTED_PROXIES));
        for entry in entries {
            if networks.len() == MAX_TRUSTED_PROXIES {
                error!("Only the first {MAX_TRUSTED_PROXIES} trusted_proxies entries are used.");
                break;
            }
            match entry.parse::<IpNet>() {
                Ok(network) => networks.push(network),
                Err(msg) => error!("Ignoring a trusted_proxies entry: {msg}"),
            }
        }
        debug_assert!(networks.len() <= MAX_TRUSTED_PROXIES);
        Self {
            networks: networks.into_boxed_slice(),
        }
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        self.networks
            .iter()
            .any(|network| network.contains(address))
    }
}

/// Entries trimmed, with blank ones dropped. The config file and the API both store them so.
pub fn trimmed_entries<'a>(entries: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    entries
        .into_iter()
        .map(str::trim)
        .filter(|entry| entry.is_empty().not())
        .map(str::to_string)
        .collect()
}

/// An address, or a range written as an address and a prefix length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNet {
    /// Host bits already cleared, in canonical form.
    network: IpAddr,
    prefix_len: u8,
}

impl IpNet {
    pub fn contains(self, address: IpAddr) -> bool {
        match (self.network, address.to_canonical()) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                mask_v4(address, self.prefix_len) == network
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                mask_v6(address, self.prefix_len) == network
            }
            _ => false,
        }
    }

    /// Clears host bits, and folds an IPv4-mapped range into IPv4, since the addresses it
    /// is checked against are canonical.
    fn canonical(address: IpAddr, prefix_len: u8) -> Self {
        match address {
            IpAddr::V4(v4) => Self {
                network: IpAddr::V4(mask_v4(v4, prefix_len)),
                prefix_len,
            },
            IpAddr::V6(v6) => {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    if let Some(v4_prefix_len) = prefix_len.checked_sub(96) {
                        return Self::canonical(IpAddr::V4(v4), v4_prefix_len);
                    }
                }
                Self {
                    network: IpAddr::V6(mask_v6(v6, prefix_len)),
                    prefix_len,
                }
            }
        }
    }
}

impl FromStr for IpNet {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let (address, prefix) = match trimmed.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (trimmed, None),
        };
        let address: IpAddr = address
            .parse()
            .map_err(|_| format!("'{value}' is not an IP address or CIDR range."))?;
        let max_prefix_len: u8 = if address.is_ipv4() { 32 } else { 128 };
        let prefix_len = match prefix {
            None => max_prefix_len,
            Some(prefix) => prefix
                .parse::<u8>()
                .ok()
                .filter(|prefix_len| *prefix_len <= max_prefix_len)
                .ok_or_else(|| format!("'{value}' has an invalid prefix length."))?,
        };
        Ok(Self::canonical(address, prefix_len))
    }
}

fn mask_v4(address: Ipv4Addr, prefix_len: u8) -> Ipv4Addr {
    debug_assert!(prefix_len <= 32);
    let mask = u32::MAX
        .checked_shl(u32::BITS - u32::from(prefix_len))
        .unwrap_or(0);
    Ipv4Addr::from(u32::from(address) & mask)
}

fn mask_v6(address: Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    debug_assert!(prefix_len <= 128);
    let mask = u128::MAX
        .checked_shl(u128::BITS - u32::from(prefix_len))
        .unwrap_or(0);
    Ipv6Addr::from(u128::from(address) & mask)
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

    fn net(value: &str) -> IpNet {
        value.parse().unwrap()
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    fn trusted(entries: &[&str]) -> TrustedProxies {
        let entries: Vec<String> = entries.iter().map(|entry| (*entry).to_string()).collect();
        TrustedProxies::from_config(&entries)
    }

    fn forwarded(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(X_FORWARDED_FOR, value.parse().unwrap());
        }
        headers
    }

    /// Goal: single addresses and CIDR ranges of both families parse, with host bits
    /// cleared, and an IPv4-mapped range folds into IPv4.
    #[test]
    fn ip_nets_parse() {
        assert!(net("10.0.0.1").contains(ip("10.0.0.1")));
        assert!(net("10.0.0.1").contains(ip("10.0.0.2")).not());
        assert!(net("172.17.5.9/16").contains(ip("172.17.200.1")));
        assert!(net("172.17.5.9/16").contains(ip("172.18.0.1")).not());
        assert!(net(" 2001:db8::/32 ").contains(ip("2001:db8:ffff::1")));
        assert!(net("::ffff:10.0.0.0/104").contains(ip("10.9.9.9")));
        assert_eq!(net("::ffff:10.0.0.0/104"), net("10.0.0.0/8"));
    }

    /// Goal: malformed entries are refused with a message, never parsed into something
    /// broader than intended.
    #[test]
    fn invalid_ip_nets_are_refused() {
        for value in [
            "",
            "proxy.lan",
            "10.0.0.0/33",
            "::/129",
            "10.0.0.0/x",
            "10.0.0.0/",
        ] {
            assert!(value.parse::<IpNet>().is_err(), "{value:?}");
        }
    }

    /// Goal: a range matches only its own family, a mapped peer matches its IPv4 range, and
    /// a zero-length prefix matches its whole family.
    #[test]
    fn ip_net_containment() {
        assert!(net("127.0.0.1").contains(ip("::ffff:127.0.0.1")));
        assert!(net("0.0.0.0/0").contains(ip("203.0.113.1")));
        assert!(net("0.0.0.0/0").contains(ip("2001:db8::1")).not());
        assert!(net("::/0").contains(ip("2001:db8::1")));
        assert!(net("::1").contains(ip("127.0.0.1")).not());
    }

    /// Goal: config parsing keeps the valid entries, drops the invalid ones, and uses no
    /// more than `MAX_TRUSTED_PROXIES` of them.
    #[test]
    fn trusted_proxies_skip_invalid_config_entries() {
        let proxies = trusted(&["10.0.0.1", "not an address", "172.17.0.0/16"]);
        assert!(proxies.contains(ip("10.0.0.1")));
        assert!(proxies.contains(ip("172.17.0.9")));
        assert!(proxies.contains(ip("192.0.2.1")).not());
        let mut entries: Vec<String> = (0..MAX_TRUSTED_PROXIES)
            .map(|index| format!("10.0.{index}.1"))
            .collect();
        entries.push("192.0.2.1".to_string());
        let capped = TrustedProxies::from_config(&entries);
        assert!(capped.contains(ip("10.0.0.1")));
        assert!(capped.contains(ip("192.0.2.1")).not());
    }

    /// Goal: a peer that is not a trusted proxy is its own client, whatever it forwards.
    #[test]
    fn untrusted_peer_header_is_ignored() {
        let proxies = trusted(&["10.0.0.1"]);
        let headers = forwarded(&["203.0.113.9"]);
        assert_eq!(
            resolve_client(ip("192.0.2.5"), &headers, &proxies),
            ip("192.0.2.5")
        );
        assert_eq!(
            resolve_client(ip("192.0.2.5"), &headers, &TrustedProxies::default()),
            ip("192.0.2.5")
        );
    }

    /// Goal: behind one trusted proxy the forwarded address is the client, and a value the
    /// client prepended itself is never reached.
    #[test]
    fn single_trusted_hop_names_the_client() {
        let proxies = trusted(&["10.0.0.1"]);
        let honest = forwarded(&["203.0.113.9"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &honest, &proxies),
            ip("203.0.113.9")
        );
        let spoofed = forwarded(&["198.51.100.1, 203.0.113.9"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &spoofed, &proxies),
            ip("203.0.113.9")
        );
    }

    /// Goal: a chain of trusted proxies is walked through, across header lines too, to the
    /// first untrusted address.
    #[test]
    fn chained_trusted_hops_are_skipped() {
        let proxies = trusted(&["10.0.0.0/8"]);
        let headers = forwarded(&["198.51.100.1, 203.0.113.9", "10.0.0.7"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &headers, &proxies),
            ip("203.0.113.9")
        );
        let all_trusted = forwarded(&["10.0.0.9, 10.0.0.7"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &all_trusted, &proxies),
            ip("10.0.0.9")
        );
    }

    /// Goal: a trusted proxy that forwards nothing, or garbage nearest to it, leaves the
    /// request keyed on the proxy rather than on anything a client wrote.
    #[test]
    fn missing_or_garbage_hops_fall_back_to_the_proxy() {
        let proxies = trusted(&["10.0.0.1"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &HeaderMap::new(), &proxies),
            ip("10.0.0.1")
        );
        let garbage = forwarded(&["203.0.113.9, unknown"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &garbage, &proxies),
            ip("10.0.0.1")
        );
    }

    /// Goal: garbage behind a chain stops the walk at the last trusted hop reached, not at
    /// the TCP peer, so the request is keyed on the proxy that received the unknown client.
    #[test]
    fn garbage_behind_a_chain_falls_back_to_the_nearest_trusted_hop() {
        let proxies = trusted(&["10.0.0.1", "10.0.0.7"]);
        let headers = forwarded(&["garbage, 10.0.0.7"]);
        assert_eq!(
            resolve_client(ip("10.0.0.1"), &headers, &proxies),
            ip("10.0.0.7")
        );
    }

    /// Goal: IPv6 hops parse bare, bracketed, and with a port, as proxies variously write them.
    #[test]
    fn ipv6_and_ported_hops_parse() {
        let proxies = trusted(&["::1"]);
        for hop in ["2001:db8::9", "[2001:db8::9]", "[2001:db8::9]:4711"] {
            let headers = forwarded(&[hop]);
            assert_eq!(
                resolve_client(ip("::1"), &headers, &proxies),
                ip("2001:db8::9"),
                "{hop}"
            );
        }
        let headers = forwarded(&["203.0.113.9:4711"]);
        assert_eq!(
            resolve_client(ip("::1"), &headers, &proxies),
            ip("203.0.113.9")
        );
    }

    /// Goal: behind a same-host proxy, the remote client is no longer this host, so it gets
    /// its own budget and faces the remote breaker instead of sharing the loopback key.
    #[test]
    fn same_host_proxy_client_is_remote() {
        let proxies = trusted(&["127.0.0.1", "::1"]);
        let headers = forwarded(&["203.0.113.9"]);
        let client = resolve_client(ip("127.0.0.1"), &headers, &proxies);
        assert_ne!(PeerKey::from_ip(client), PeerKey::Loopback);
        let direct = resolve_client(ip("127.0.0.1"), &HeaderMap::new(), &proxies);
        assert_eq!(PeerKey::from_ip(direct), PeerKey::Loopback);
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
