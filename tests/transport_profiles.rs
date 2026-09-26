use std::time::{Duration, Instant};

use vesper3d::viewer::{
    net::{DatagramTransport, Identity, Packet, SecureSocket, TransportProfile, PROTOCOL_VERSION},
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

#[test]
fn transport_profiles_have_unambiguous_deployment_names() {
    assert_eq!(
        "development".parse::<TransportProfile>().unwrap(),
        TransportProfile::Development
    );
    assert_eq!(
        "production".parse::<TransportProfile>().unwrap(),
        TransportProfile::Production
    );
    assert!("auto".parse::<TransportProfile>().is_err());
}

#[test]
fn authoritative_handshake_runs_over_quic() {
    let generated = rcgen::generate_simple_self_signed(vec!["feta.local".into()]).unwrap();
    let certificate = generated.cert.der().to_vec();
    let identity = Identity::from_der(certificate.clone(), generated.signing_key.serialize_der());
    let transport = SecureSocket::server("127.0.0.1:0".parse().unwrap(), identity).unwrap();
    let address = transport.local_addr();
    let world = HeadlessWorld::new().unwrap();
    let content_hash = world.content_hash;
    let mut server = DedicatedServer::with_transport(transport, world).unwrap();
    let mut client = SecureSocket::client(address, certificate).unwrap();

    client
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
                content_hash,
            },
            address,
        )
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut welcomed = false;
    while Instant::now() < deadline {
        server.poll_network().unwrap();
        for (packet, peer) in client.receive_packets().unwrap() {
            if peer == address && matches!(packet, Packet::Welcome { player_id: 1, .. }) {
                welcomed = true;
            }
        }
        if welcomed {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(
        welcomed,
        "QUIC client did not receive the authoritative welcome"
    );
    assert_eq!(server.sessions.len(), 1);
}
