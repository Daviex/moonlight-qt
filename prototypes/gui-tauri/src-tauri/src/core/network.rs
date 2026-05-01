#![allow(dead_code)]

use super::error::CoreError;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub const GAMESTREAM_TCP_PORTS: &[u16] = &[47984, 47989, 48010];
pub const GAMESTREAM_UDP_PORTS: &[u16] = &[47998, 47999, 48000, 48002];

const WAKE_PACKET_LENGTH: usize = 102;
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortDiagnostic {
    pub port: u16,
    pub reachable: bool,
}

pub fn build_wake_packet(mac_address: &str) -> Result<[u8; WAKE_PACKET_LENGTH], CoreError> {
    let mac = parse_mac_address(mac_address)?;
    let mut packet = [0xff_u8; WAKE_PACKET_LENGTH];
    for chunk in packet[6..].chunks_exact_mut(mac.len()) {
        chunk.copy_from_slice(&mac);
    }
    Ok(packet)
}

pub fn send_wake_packet(mac_address: &str) -> Result<(), CoreError> {
    let packet = build_wake_packet(mac_address)?;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|error| {
        CoreError::Backend(format!("Unable to bind Wake-on-LAN socket: {error}"))
    })?;
    socket.set_broadcast(true).map_err(|error| {
        CoreError::Backend(format!("Unable to enable Wake-on-LAN broadcast: {error}"))
    })?;
    socket
        .send_to(&packet, "255.255.255.255:9")
        .map_err(|error| {
            CoreError::Backend(format!("Unable to send Wake-on-LAN packet: {error}"))
        })?;
    Ok(())
}

pub fn diagnose_tcp_ports(address: &str, ports: &[u16]) -> Vec<PortDiagnostic> {
    ports
        .iter()
        .copied()
        .map(|port| PortDiagnostic {
            port,
            reachable: tcp_port_reachable(address, port, DEFAULT_CONNECT_TIMEOUT),
        })
        .collect()
}

pub fn blocked_ports(diagnostics: &[PortDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| !diagnostic.reachable)
        .map(|diagnostic| diagnostic.port.to_string())
        .collect()
}

fn tcp_port_reachable(address: &str, port: u16, timeout: Duration) -> bool {
    let Ok(socket_addrs) = (address, port).to_socket_addrs() else {
        return false;
    };

    socket_addrs
        .filter(is_supported_socket_addr)
        .any(|socket_addr| TcpStream::connect_timeout(&socket_addr, timeout).is_ok())
}

fn is_supported_socket_addr(socket_addr: &SocketAddr) -> bool {
    socket_addr.is_ipv4() || socket_addr.is_ipv6()
}

fn parse_mac_address(mac_address: &str) -> Result<[u8; 6], CoreError> {
    let hex: String = mac_address
        .chars()
        .filter(|character| *character != ':' && *character != '-')
        .collect();
    if hex.len() != 12 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(CoreError::Validation(
            "Wake-on-LAN requires a 6-byte MAC address.".into(),
        ));
    }

    let mut mac = [0_u8; 6];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let value = std::str::from_utf8(chunk)
            .ok()
            .and_then(|part| u8::from_str_radix(part, 16).ok())
            .ok_or_else(|| CoreError::Validation("MAC address contains invalid hex.".into()))?;
        mac[index] = value;
    }
    Ok(mac)
}

#[cfg(test)]
mod tests {
    use super::{blocked_ports, build_wake_packet, PortDiagnostic};

    #[test]
    fn wake_packet_repeats_mac_after_sync_stream() {
        let packet = build_wake_packet("00:11:22:33:44:55").unwrap();

        assert_eq!([0xff; 6], packet[..6]);
        assert_eq!([0x00, 0x11, 0x22, 0x33, 0x44, 0x55], packet[6..12]);
        assert_eq!([0x00, 0x11, 0x22, 0x33, 0x44, 0x55], packet[96..102]);
    }

    #[test]
    fn wake_packet_accepts_dash_and_plain_mac_forms() {
        assert_eq!(
            build_wake_packet("00-11-22-33-44-55").unwrap(),
            build_wake_packet("001122334455").unwrap()
        );
    }

    #[test]
    fn wake_packet_rejects_invalid_mac() {
        let error = build_wake_packet("00:11:22").unwrap_err();

        assert_eq!(
            "Wake-on-LAN requires a 6-byte MAC address.",
            error.to_string()
        );
    }

    #[test]
    fn blocked_ports_formats_unreachable_ports() {
        let blocked = blocked_ports(&[
            PortDiagnostic {
                port: 47984,
                reachable: true,
            },
            PortDiagnostic {
                port: 48010,
                reachable: false,
            },
        ]);

        assert_eq!(vec!["48010"], blocked);
    }
}
