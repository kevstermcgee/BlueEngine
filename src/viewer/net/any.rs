//! One transport type for either engine socket, chosen at run time.
//!
//! [`AnyTransport`] wraps a boxed [`DatagramTransport`]; [`server_transport`] and [`client_transport`] build
//! the right one for a [`TransportProfile`]: raw UDP for development, QUIC over TLS 1.3 with the pinned
//! certificate for production (`BLUE_TLS_KEY_FILE` / `BLUE_TLS_CERT_FILE`, see `docs/HOSTING.md`). Games with
//! their own protocol use these instead of copying the engine binaries' setup code.
use super::quic::{trusted_certificate, Identity, SecureSocket};
use super::transport::{Datagram, DatagramTransport, PeerId, TransportProfile};
use super::UdpTransport;
use std::net::SocketAddr;

/// Either engine transport behind one type.
pub struct AnyTransport(Box<dyn DatagramTransport>);

impl AnyTransport {
    pub fn new(inner: impl DatagramTransport + 'static) -> Self {
        Self(Box::new(inner))
    }
}

impl DatagramTransport for AnyTransport {
    fn send(&self, peer: PeerId, data: &[u8]) -> crate::Result<usize> {
        self.0.send(peer, data)
    }
    fn payload_limit(&self, peer: PeerId) -> usize {
        self.0.payload_limit(peer)
    }
    fn receive(&mut self) -> crate::Result<Vec<Datagram>> {
        self.0.receive()
    }
    fn local_addr(&self) -> crate::Result<SocketAddr> {
        self.0.local_addr()
    }
}

/// A server socket bound to `address`.
pub fn server_transport(profile: TransportProfile, address: &str) -> crate::Result<AnyTransport> {
    Ok(match profile {
        TransportProfile::Development => AnyTransport::new(UdpTransport::bind(address)?),
        TransportProfile::Production => {
            AnyTransport::new(SecureSocket::server(address.parse()?, Identity::load()?)?)
        }
    })
}

/// A client socket toward `server`.
pub fn client_transport(
    profile: TransportProfile,
    server: SocketAddr,
) -> crate::Result<AnyTransport> {
    Ok(match profile {
        TransportProfile::Development => {
            AnyTransport::new(UdpTransport::bind(if server.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })?)
        }
        TransportProfile::Production => {
            AnyTransport::new(SecureSocket::client(server, trusted_certificate()?)?)
        }
    })
}
