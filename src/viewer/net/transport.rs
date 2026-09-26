//! Generic datagram transport abstraction for game networking.
//!
//! Packet serialization lives here, above concrete sockets, so authoritative
//! gameplay never needs to know whether a datagram came from development UDP or
//! production QUIC/TLS.
use super::Packet;
use std::net::SocketAddr;

pub type PeerId = SocketAddr;

/// An incoming or outgoing network datagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Datagram {
    pub peer: PeerId,
    pub data: Vec<u8>,
}

/// Abstract datagram transport interface.
/// Allows swapping between raw UDP, secure QUIC/TLS 1.3, or simulated loopback transports.
pub trait DatagramTransport {
    /// Send raw datagram bytes to a remote peer.
    fn send(&self, peer: PeerId, data: &[u8]) -> crate::Result<usize>;

    /// Poll all available incoming datagrams without blocking.
    fn receive(&mut self) -> crate::Result<Vec<Datagram>>;

    /// The local socket address this transport is bound to.
    fn local_addr(&self) -> crate::Result<SocketAddr>;

    /// Encode and send one protocol packet over this transport.
    fn send_packet(&self, packet: &Packet, peer: PeerId) -> crate::Result<usize> {
        self.send(peer, &packet.encode()?)
    }

    /// Receive and decode all currently available protocol packets.
    ///
    /// Malformed remote datagrams are dropped. Transport failures still propagate.
    fn receive_packets(&mut self) -> crate::Result<Vec<(Packet, PeerId)>> {
        Ok(self
            .receive()?
            .into_iter()
            .filter_map(|datagram| {
                Packet::decode(&datagram.data)
                    .ok()
                    .map(|packet| (packet, datagram.peer))
            })
            .collect())
    }
}

/// User-facing deployment profiles for the two executable transports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransportProfile {
    /// Raw, unencrypted UDP for local development, tests, and impairment tooling.
    #[default]
    Development,
    /// QUIC datagrams protected by TLS 1.3 with a pinned server certificate.
    Production,
}

impl std::str::FromStr for TransportProfile {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "development" | "dev" | "udp" => Ok(Self::Development),
            "production" | "prod" | "quic" => Ok(Self::Production),
            _ => Err("transport must be development (UDP) or production (QUIC/TLS)"),
        }
    }
}

impl std::fmt::Display for TransportProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Development => formatter.write_str("development/udp"),
            Self::Production => formatter.write_str("production/quic"),
        }
    }
}
