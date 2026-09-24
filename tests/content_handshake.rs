use vesper3d::math::V;
use vesper3d::viewer::{
    content::fingerprint,
    controller::Collider,
    maps::{build, MapId},
    net::{Packet, UdpTransport, PROTOCOL_VERSION},
    server::DedicatedServer,
    spatial::{RoomGraph, RoomId},
};

#[test]
fn content_hash_is_stable_and_covers_collision_and_graph() {
    let mut room = build(MapId::TestLab).unwrap();
    let hash = fingerprint(&room);
    for _ in 0..8 {
        assert_eq!(hash, fingerprint(&build(MapId::TestLab).unwrap()));
    }
    room.colliders[0].min.0 -= 1.;
    assert_ne!(hash, fingerprint(&room));
    let mut room = build(MapId::TestLab).unwrap();
    room.spatial.as_mut().unwrap().portals[0].is_open = false;
    assert_ne!(hash, fingerprint(&room));
}

#[test]
fn mismatch_and_capacity_rejections_do_not_create_sessions() {
    let mut server = DedicatedServer::bind("127.0.0.1:0").unwrap();
    let client = UdpTransport::bind("127.0.0.1:0").unwrap();
    let address = client.local_addr().unwrap();
    let hash = server.world.content_hash;
    server.handle_hello(address, PROTOCOL_VERSION, 0, hash ^ 1);
    assert!(server.sessions.is_empty());
    assert!(server.world.player(1).is_none());
    server.handle_hello(address, PROTOCOL_VERSION - 1, 0, hash);
    assert!(server.sessions.is_empty());
    for port in 40001..=40008 {
        server.handle_hello(
            format!("127.0.0.1:{port}").parse().unwrap(),
            PROTOCOL_VERSION,
            0,
            hash,
        );
    }
    assert_eq!(server.sessions.len(), 8);
    server.handle_hello(address, PROTOCOL_VERSION, 0, hash);
    assert_eq!(server.sessions.len(), 8);
    assert!(!server.clients.contains_key(&address));
    assert!(server.world.player(9).is_none());
}

#[test]
fn old_or_garbage_packets_fail_closed() {
    assert!(Packet::decode(br#"{"Hello":{"protocol_version":1,"player_id":0}}"#).is_err());
    assert!(Packet::decode(&vec![0; 1401]).is_err());
    for size in 0..=1400 {
        assert!(Packet::decode(&vec![0xff; size]).is_err());
    }
    let bytes = Packet::Hello {
        protocol_version: PROTOCOL_VERSION,
        player_id: 0,
        content_hash: 123,
    }
    .encode()
    .unwrap();
    for size in 0..bytes.len() {
        assert!(Packet::decode(&bytes[..size]).is_err());
    }
}

#[test]
fn overlapping_rooms_choose_smallest_stable_id() {
    for _ in 0..20 {
        let mut graph = RoomGraph::new();
        for id in [3, 1, 2] {
            graph.add_room(
                RoomId(id),
                "overlap",
                Collider {
                    min: V::ZERO,
                    max: V::ONE,
                },
                0,
            );
        }
        assert_eq!(graph.find_room_at(V(0.5, 0.5, 0.5)), Some(RoomId(1)));
    }
}

#[test]
fn garbage_datagrams_do_not_kill_server_or_accept_truncated_prefixes() {
    let mut server = DedicatedServer::bind("127.0.0.1:0").unwrap();
    let mut client = UdpTransport::bind("127.0.0.1:0").unwrap();
    client
        .socket
        .send_to(b"not json", server.local_addr)
        .unwrap();
    let hello = Packet::Hello {
        protocol_version: PROTOCOL_VERSION,
        player_id: 0,
        content_hash: server.world.content_hash,
    };
    let mut oversized = hello.encode().unwrap();
    oversized.resize(2000, b' ');
    client
        .socket
        .send_to(&oversized, server.local_addr)
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        server.poll_network().unwrap();
        if server.transport.socket.peek_from(&mut [0; 1]).is_err() {
            break;
        }
    }
    assert!(server.sessions.is_empty());
    // The next ordinary handshake still succeeds, proving the host stays usable.
    client.send_packet(&hello, server.local_addr).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let mut welcomed = false;
    while std::time::Instant::now() < deadline {
        server.poll_network().unwrap();
        if let Some((Packet::Welcome { .. }, _)) = client.recv_packet().unwrap() {
            welcomed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(welcomed);
    assert_eq!(server.sessions.len(), 1);
}
