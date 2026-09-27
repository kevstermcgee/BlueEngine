//! Real-socket liveness only. Deterministic impairments belong in replication_budget.
use std::time::{Duration, Instant};
use vesper3d::{
    math::V,
    viewer::{
        lifecycle::LifecycleState, net::*, server::DedicatedServer, simulation::HeadlessWorld,
    },
};
fn run<T: DatagramTransport>(transport: T, mut clients: Vec<Box<dyn DatagramTransport>>) {
    let mut world = HeadlessWorld::new().unwrap();
    for i in 0..40 {
        let idx = world
            .lifecycle
            .register(format!("replica-{i}"), "replica".into(), V(0., 1., 0.));
        world
            .lifecycle
            .promote(idx, LifecycleState::ReplicatedEntity);
    }
    let hash = world.content_hash;
    let mut server = DedicatedServer::with_transport(transport, world).unwrap();
    let address = server.local_addr;
    let mut tokens = [None; 2];
    let mut states = vec![None; 2];
    for client in &clients {
        client
            .send_packet(
                &Packet::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    player_id: 0,
                    content_hash: hash,
                },
                address,
            )
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut tick = 0;
    while Instant::now() < deadline {
        tick += 1;
        server.poll_network().unwrap();
        server.world.tick = tick;
        // Keep the world stable: this test measures transport convergence, not physics.
        if tick % 3 == 0 {
            server.try_broadcast_snapshots().unwrap();
        }
        for (i, client) in clients.iter_mut().enumerate() {
            for (packet, peer) in client.receive_packets().unwrap() {
                assert_eq!(peer, address);
                match packet {
                    Packet::Welcome { session_token, .. } => tokens[i] = session_token,
                    p @ (Packet::Snapshot(_) | Packet::Delta(_)) => {
                        receive_update(&mut states[i], p).unwrap();
                    }
                    _ => {}
                }
            }
            if let Some(token) = tokens[i] {
                client
                    .send_packet(
                        &Packet::Input(InputFrame {
                            client_tick: tick,
                            ack_server_tick: states[i].as_ref().map_or(0, |s| s.tick),
                            session_token: Some(token),
                            ..Default::default()
                        }),
                        address,
                    )
                    .unwrap();
            }
        }
        if states.iter().all(|s| {
            s.as_ref().is_some_and(|s| {
                s.props
                    .iter()
                    .filter(|p| p.id.starts_with("replica-"))
                    .count()
                    == 40
                    && s.players.len() == 2
            })
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("two real-socket clients did not converge within five seconds");
}
#[test]
fn udp_two_clients_receive_multi_packet_world() {
    run(
        UdpTransport::bind("127.0.0.1:0").unwrap(),
        (0..2)
            .map(|_| {
                Box::new(UdpTransport::bind("127.0.0.1:0").unwrap()) as Box<dyn DatagramTransport>
            })
            .collect(),
    );
}
#[test]
fn quic_two_clients_receive_multi_packet_world() {
    let cert = rcgen::generate_simple_self_signed(vec!["feta.local".into()]).unwrap();
    let der = cert.cert.der().to_vec();
    let socket = SecureSocket::server(
        "127.0.0.1:0".parse().unwrap(),
        Identity::from_der(der.clone(), cert.signing_key.serialize_der()),
    )
    .unwrap();
    let address = socket.local_addr();
    run(
        socket,
        (0..2)
            .map(|_| {
                Box::new(SecureSocket::client(address, der.clone()).unwrap())
                    as Box<dyn DatagramTransport>
            })
            .collect(),
    );
}
