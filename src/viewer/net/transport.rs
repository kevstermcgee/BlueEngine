//! Generic datagram transport abstraction for game networking.
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
}
