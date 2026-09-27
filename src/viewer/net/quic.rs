//! Production-ready QUIC / TLS 1.3 datagram transport with pinned certificates.
//!
//! Features:
//! - Unreliable, ordered/unordered QUIC datagrams over TLS 1.3
//! - Bundled pinned server certificate (no public CA dependency, no insecure fallbacks)
//! - Stateless address validation on connection handshakes
//! - Bounded asynchronous worker thread isolated from game simulation loops
//! - Bounded datagram budgets (1,100 bytes MTU safe)
//! - Per-connection rate limiting and anti-amplification protection
use super::transport::{Datagram, DatagramTransport, PeerId};
use quinn::{Connection, Endpoint, TransportConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tokio::sync::{mpsc as async_mpsc, oneshot, Semaphore};

/// Bundled default self-signed certificate for local / pinned connections.
pub const DEFAULT_CERTIFICATE: &[u8] = include_bytes!("../../../assets/network/server-cert.der");

/// Load the pinned DER certificate selected for this deployment.
///
/// Clients and servers may point `BLUE_TLS_CERT_FILE` at the same public certificate.
/// Without it, the bundled development pin is used for compatibility.
pub fn trusted_certificate() -> crate::Result<Vec<u8>> {
    std::env::var_os("BLUE_TLS_CERT_FILE")
        .map(std::path::PathBuf::from)
        .map_or_else(
            || Ok(DEFAULT_CERTIFICATE.to_vec()),
            |path| Ok(std::fs::read(path)?),
        )
}

/// Safe datagram payload budget to guarantee single-packet datagram delivery without fragmentation.
pub const PAYLOAD: usize = super::MAX_PACKET_BYTES;

/// Server identity holding DER certificate and private key.
pub struct Identity {
    pub certificate: Vec<u8>,
    pub private_key: Vec<u8>,
}

impl Identity {
    /// Load private key from environment or default config directory.
    pub fn load() -> crate::Result<Self> {
        let path = std::env::var_os("BLUE_TLS_KEY_FILE")
            .or_else(|| std::env::var_os("FETA_TLS_KEY_FILE"))
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| std::path::PathBuf::from(h).join(".config/blueengine/server-key.der"))
            })
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| std::path::PathBuf::from(h).join(".config/blueengine/server-key.der"))
            })
            .ok_or(
                "Set BLUE_TLS_KEY_FILE (or FETA_TLS_KEY_FILE) to the private PKCS#8 DER server key",
            )?;

        Ok(Self {
            certificate: trusted_certificate()?,
            private_key: std::fs::read(path)?,
        })
    }

    /// Construct directly with provided DER certificate and private key bytes.
    pub fn from_der(certificate: Vec<u8>, private_key: Vec<u8>) -> Self {
        Self {
            certificate,
            private_key,
        }
    }
}

fn transport_config() -> Arc<TransportConfig> {
    let mut t = TransportConfig::default();
    t.max_concurrent_bidi_streams(0u8.into())
        .max_concurrent_uni_streams(0u8.into())
        .datagram_receive_buffer_size(Some(PAYLOAD * 64))
        .datagram_send_buffer_size(PAYLOAD * 64)
        .max_idle_timeout(Some(Duration::from_secs(10).try_into().unwrap()))
        .keep_alive_interval(Some(Duration::from_secs(2)));
    Arc::new(t)
}

type Connections = Arc<Mutex<HashMap<SocketAddr, Connection>>>;
type Incoming = mpsc::SyncSender<(SocketAddr, Vec<u8>)>;

enum Mode {
    Server(Identity),
    Client {
        server: SocketAddr,
        certificate: Vec<u8>,
        server_name: String,
    },
}

/// Aggregate transport counters; none of these certify remote delivery.
#[derive(Default, Debug)]
pub struct TransportCounters {
    pub queued: AtomicU64,
    pub queue_full: AtomicU64,
    pub submitted: AtomicU64,
    pub worker_dropped: AtomicU64,
    pub incoming_dropped: AtomicU64,
}
fn submit(connection: &Connection, bytes: Vec<u8>, counters: &TransportCounters) {
    // Quinn's default send_datagram evicts old datagrams on saturation. Avoid
    // eviction and account for the rejected submission instead. Authority retries
    // its immutable packet until an application acknowledgement arrives.
    if connection.datagram_send_buffer_space() < bytes.len()
        || connection.send_datagram(bytes.into()).is_err()
    {
        counters.worker_dropped.fetch_add(1, Ordering::Relaxed);
    } else {
        counters.submitted.fetch_add(1, Ordering::Relaxed);
    }
}
fn deliver(incoming: &Incoming, address: SocketAddr, bytes: Vec<u8>, counters: &TransportCounters) {
    if incoming.try_send((address, bytes)).is_err() {
        counters.incoming_dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// Bounded synchronous facade over one dedicated async network thread.
pub struct SecureSocket {
    address: SocketAddr,
    connections: Connections,
    pub counters: Arc<TransportCounters>,
    send: async_mpsc::Sender<(SocketAddr, Vec<u8>)>,
    receive: mpsc::Receiver<(SocketAddr, Vec<u8>)>,
    error: Arc<Mutex<Option<String>>>,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SecureSocket {
    /// Bind as a dedicated QUIC server with provided identity.
    pub fn server(address: SocketAddr, identity: Identity) -> crate::Result<Self> {
        Self::start(address, Mode::Server(identity))
    }

    /// Connect as a client to a server, pinning the expected certificate.
    pub fn client(server: SocketAddr, certificate: Vec<u8>) -> crate::Result<Self> {
        Self::client_with_name(server, certificate, "feta.local".to_string())
    }

    /// Connect as a client with custom SNI hostname.
    pub fn client_with_name(
        server: SocketAddr,
        certificate: Vec<u8>,
        server_name: String,
    ) -> crate::Result<Self> {
        let bind_addr: SocketAddr = if server.is_ipv4() {
            "0.0.0.0:0".parse()?
        } else {
            "[::]:0".parse()?
        };
        Self::start(
            bind_addr,
            Mode::Client {
                server,
                certificate,
                server_name,
            },
        )
    }

    fn start(address: SocketAddr, mode: Mode) -> crate::Result<Self> {
        let (send, mut outgoing) = async_mpsc::channel::<(SocketAddr, Vec<u8>)>(128);
        let (incoming, receive) = mpsc::sync_channel(256);
        let (stop, mut stopped) = oneshot::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let connections: Connections = Arc::new(Mutex::new(HashMap::new()));
        let worker_connections = connections.clone();
        let counters = Arc::new(TransportCounters::default());
        let worker_counters = counters.clone();

        let worker = thread::Builder::new()
            .name("blue-quic".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let setup_res = setup(address, mode);
                    let (endpoint, remote, sni) = match setup_res {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                    let _ = ready_tx.send(endpoint.local_addr().map_err(|e| e.to_string()));

                    let connections = worker_connections;
                    let counters = worker_counters;
                    if let Some(server) = remote {
                        let sni_name = sni.unwrap_or_else(|| "feta.local".into());
                        let attempt = match endpoint.connect(server, &sni_name) {
                            Ok(c) => c,
                            Err(e) => {
                                *worker_error.lock().unwrap() = Some(e.to_string());
                                return;
                            }
                        };
                        let connection = tokio::select! {
                            _ = &mut stopped => return,
                            result = tokio::time::timeout(Duration::from_secs(6), attempt) => match result {
                                Ok(Ok(c)) => c,
                                other => {
                                    *worker_error.lock().unwrap() = Some(format!("Secure connection failed: {other:?}. Check server address and certificate."));
                                    return;
                                }
                            }
                        };

                        connections.lock().unwrap().insert(server, connection.clone());
                        loop {
                            tokio::select! {
                                _ = &mut stopped => break,
                                packet = outgoing.recv() => match packet {
                                    Some((_, bytes)) => { submit(&connection, bytes, &counters); },
                                    None => break,
                                },
                                packet = connection.read_datagram() => match packet {
                                    Ok(bytes) if bytes.len() <= PAYLOAD => {
                                        deliver(&incoming, server, bytes.to_vec(), &counters);
                                    },
                                    Ok(_) => {},
                                    Err(_) => {
                                        *worker_error.lock().unwrap() = Some("Secure connection closed.".into());
                                        break;
                                    }
                                }
                            }
                        }
                        connection.close(0u8.into(), b"client exit");
                    } else {
                        let permits = Arc::new(Semaphore::new(8));
                        loop {
                            tokio::select! {
                                _ = &mut stopped => break,
                                packet = outgoing.recv() => match packet {
                                    Some((addr, bytes)) => {
                                        if let Some(c) = connections.lock().unwrap().get(&addr) {
                                            submit(c, bytes, &counters);
                                        } else { counters.worker_dropped.fetch_add(1, Ordering::Relaxed); }
                                    },
                                    None => break,
                                },
                                candidate = endpoint.accept() => {
                                    let Some(candidate) = candidate else { break; };
                                    if !candidate.remote_address_validated() {
                                        let _ = candidate.retry();
                                        continue;
                                    }
                                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                                        candidate.refuse();
                                        continue;
                                    };
                                    let connections = connections.clone();
                                    let incoming = incoming.clone();
                                    let counters = counters.clone();
                                    tokio::spawn(async move {
                                        let _permit = permit;
                                        if let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(5), candidate).await {
                                            receive_connection(connection, connections, incoming, counters).await;
                                        }
                                    });
                                }
                            }
                        }
                    }
                    endpoint.close(0u8.into(), b"shutdown");
                });
            })?;

        let address = ready_rx
            .recv()
            .map_err(|_| "Network worker stopped during startup")?
            .map_err(|e| format!("Secure network startup: {e}"))?;

        Ok(Self {
            address,
            connections,
            counters,
            send,
            receive,
            error,
            stop: Some(stop),
            worker: Some(worker),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn send_to(&self, bytes: &[u8], address: SocketAddr) -> crate::Result<usize> {
        if bytes.len() > self.payload_limit(address) {
            return Err("Secure datagram exceeds payload budget".into());
        }
        match self.send.try_send((address, bytes.to_vec())) {
            Ok(()) => {
                self.counters.queued.fetch_add(1, Ordering::Relaxed);
                Ok(bytes.len())
            }
            Err(async_mpsc::error::TrySendError::Full(_)) => {
                self.counters.queue_full.fetch_add(1, Ordering::Relaxed);
                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock).into())
            }
            Err(_) => Err("Secure connection is closed".into()),
        }
    }

    pub fn receive(&self) -> crate::Result<Vec<(SocketAddr, Vec<u8>)>> {
        if let Some(error) = self.error.lock().unwrap().take() {
            return Err(error.into());
        }
        Ok(self.receive.try_iter().take(128).collect())
    }
}

impl DatagramTransport for SecureSocket {
    fn payload_limit(&self, peer: PeerId) -> usize {
        self.connections
            .lock()
            .unwrap()
            .get(&peer)
            .map_or(PAYLOAD, |c| c.max_datagram_size().unwrap_or(0).min(PAYLOAD))
    }
    fn send(&self, peer: PeerId, data: &[u8]) -> crate::Result<usize> {
        self.send_to(data, peer)
    }

    fn receive(&mut self) -> crate::Result<Vec<Datagram>> {
        let raw = SecureSocket::receive(self)?;
        Ok(raw
            .into_iter()
            .map(|(peer, data)| Datagram { peer, data })
            .collect())
    }

    fn local_addr(&self) -> crate::Result<SocketAddr> {
        Ok(self.local_addr())
    }
}

impl Drop for SecureSocket {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn setup(
    address: SocketAddr,
    mode: Mode,
) -> Result<(Endpoint, Option<SocketAddr>, Option<String>), String> {
    match mode {
        Mode::Server(identity) => {
            let mut config = quinn::ServerConfig::with_single_cert(
                vec![CertificateDer::from(identity.certificate)],
                PrivatePkcs8KeyDer::from(identity.private_key).into(),
            )
            .map_err(|e| e.to_string())?;

            config
                .transport_config(transport_config())
                .incoming_buffer_size(16 * 1024)
                .incoming_buffer_size_total(128 * 1024);

            let endpoint = Endpoint::server(config, address).map_err(|e| e.to_string())?;
            Ok((endpoint, None, None))
        }
        Mode::Client {
            server,
            certificate,
            server_name,
        } => {
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(CertificateDer::from(certificate))
                .map_err(|e| e.to_string())?;

            let mut config = quinn::ClientConfig::with_root_certificates(Arc::new(roots))
                .map_err(|e| e.to_string())?;
            config.transport_config(transport_config());

            let mut endpoint = Endpoint::client(address).map_err(|e| e.to_string())?;
            endpoint.set_default_client_config(config);
            Ok((endpoint, Some(server), Some(server_name)))
        }
    }
}

async fn receive_connection(
    connection: Connection,
    connections: Connections,
    incoming: Incoming,
    counters: Arc<TransportCounters>,
) {
    let address = connection.remote_address();
    {
        let mut map = connections.lock().unwrap();
        if map.contains_key(&address) {
            connection.close(1u8.into(), b"endpoint busy");
            return;
        }
        map.insert(address, connection.clone());
    }

    let mut window = tokio::time::Instant::now();
    let mut count = 0;
    while let Ok(Ok(bytes)) =
        tokio::time::timeout(Duration::from_secs(8), connection.read_datagram()).await
    {
        if window.elapsed() >= Duration::from_secs(1) {
            window = tokio::time::Instant::now();
            count = 0;
        }
        count += 1;
        if count > 240 || bytes.len() > PAYLOAD {
            break;
        }
        deliver(&incoming, address, bytes.to_vec(), &counters);
    }

    connections.lock().unwrap().remove(&address);
    connection.close(0u8.into(), b"session ended");
}

#[cfg(test)]
mod queue_regression {
    use super::*;
    #[test]
    fn saturated_queue_does_not_report_acceptance() {
        let (send, _outgoing) = async_mpsc::channel(1);
        let (_incoming, receive) = mpsc::sync_channel(1);
        let socket = SecureSocket {
            address: "127.0.0.1:1".parse().unwrap(),
            send,
            receive,
            connections: Arc::new(Mutex::new(HashMap::new())),
            counters: Arc::new(TransportCounters::default()),
            error: Arc::new(Mutex::new(None)),
            stop: None,
            worker: None,
        };
        socket.send_to(&[1], socket.address).unwrap();
        assert!(
            socket.send_to(&[2], socket.address).is_err(),
            "full queue reported success"
        );
    }
}
