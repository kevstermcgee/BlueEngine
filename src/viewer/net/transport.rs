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

/// Local submission outcome. Acceptance is NOT peer receipt; only a protocol ack
/// confirms that. Backpressure retains ownership at the caller for bounded retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Accepted { bytes: usize },
    Backpressured,
}

/// Abstract datagram transport interface.
/// Allows swapping between raw UDP, secure QUIC/TLS 1.3, or simulated loopback transports.
pub trait DatagramTransport {
    /// Send raw datagram bytes to a remote peer.
    fn send(&self, peer: PeerId, data: &[u8]) -> crate::Result<usize>;

    /// Active per-peer datagram payload budget, including the protocol envelope.
    fn payload_limit(&self, _peer: PeerId) -> usize {
        super::MAX_PACKET_BYTES
    }

    fn try_send(&self, peer: PeerId, data: &[u8]) -> crate::Result<SendOutcome> {
        if data.len() > self.payload_limit(peer).min(super::MAX_PACKET_BYTES) {
            return Err(format!(
                "Datagram needs {} bytes, active payload limit is {}",
                data.len(),
                self.payload_limit(peer)
            )
            .into());
        }
        match self.send(peer, data) {
            Ok(n) if n == data.len() => Ok(SendOutcome::Accepted { bytes: n }),
            Ok(_) => Err("Transport accepted a partial datagram".into()),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock) =>
            {
                Ok(SendOutcome::Backpressured)
            }
            Err(e) => Err(e),
        }
    }

    /// Poll all available incoming datagrams without blocking.
    fn receive(&mut self) -> crate::Result<Vec<Datagram>>;

    /// The local socket address this transport is bound to.
    fn local_addr(&self) -> crate::Result<SocketAddr>;

    /// Submission-aware packet send; backpressure is not a fatal connection error.
    fn try_send_packet(&self, packet: &Packet, peer: PeerId) -> crate::Result<SendOutcome> {
        self.try_send(peer, &packet.encode()?)
    }

    /// Encode and send one protocol packet over this transport.
    fn send_packet(&self, packet: &Packet, peer: PeerId) -> crate::Result<usize> {
        match self.try_send(peer, &packet.encode()?)? {
            SendOutcome::Accepted { bytes } => Ok(bytes),
            SendOutcome::Backpressured => {
                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock).into())
            }
        }
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
