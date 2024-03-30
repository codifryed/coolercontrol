/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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
 ******************************************************************************/

use rand::prelude::*;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener, ToSocketAddrs};

pub type Port = u16;

fn test_bind_tcp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}

pub fn is_free_tcp_ipv4(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    test_bind_tcp(ipv4).is_some()
}

pub fn is_free_tcp_ipv6(port: Port) -> bool {
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);
    test_bind_tcp(ipv6).is_some()
}

/// Asks the OS for a free ipv4 port
fn ask_free_tcp_port_ipv4() -> Option<Port> {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    test_bind_tcp(ipv4)
}

/// Asks the OS for a free ipv6 port
fn ask_free_tcp_port_ipv6() -> Option<Port> {
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);
    test_bind_tcp(ipv6)
}

/// This is our own custom implementation for finding a free TCP port for IPv4, IPv6, or both.
pub fn find_free_port() -> Option<Port> {
    let mut rng = rand::thread_rng();

    // Try random port for both ipvs first
    for _ in 0..10 {
        let port = rng.gen_range(15000..25000);
        if is_free_tcp_ipv4(port) && is_free_tcp_ipv6(port) {
            return Some(port);
        }
    }

    // Try random port for ipv4
    for _ in 0..10 {
        let port = rng.gen_range(15000..25000);
        if is_free_tcp_ipv4(port) {
            return Some(port);
        }
    }

    // Try random port for ipv6
    for _ in 0..10 {
        let port = rng.gen_range(15000..25000);
        if is_free_tcp_ipv6(port) {
            return Some(port);
        }
    }

    // Fallback: Ask the OS for a port for both ipv4 and ipv6
    for _ in 0..10 {
        if let Some(port) = ask_free_tcp_port_ipv4() {
            // check if the same port on ipv6 is free as well
            if is_free_tcp_ipv6(port) {
                return Some(port);
            }
        }
    }

    // Fallback: Ask the OS for a port for ipv4
    for _ in 0..10 {
        if let Some(port) = ask_free_tcp_port_ipv4() {
            return Some(port);
        }
    }

    // Fallback: Ask the OS for a port for ipv6
    for _ in 0..10 {
        if let Some(port) = ask_free_tcp_port_ipv6() {
            return Some(port);
        }
    }

    None // No free port found
}

#[cfg(test)]
mod tests {
    use super::find_free_port;

    #[test]
    fn free_port_found() {
        assert!(find_free_port().is_some());
    }
}
