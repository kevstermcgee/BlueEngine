//! Integration tests for authenticated live multiplayer and network impairment proxy.

use std::{
    thread,
    time::{Duration, Instant},
};

use vesper3d::viewer::{
    controller::Movement,
    net::{
        compute_auth_proof, proxy::NetworkProxyConfig, InputFrame, Packet, UdpProxyServer,
        UdpTransport, PROTOCOL_VERSION,
    },
    server::DedicatedServer,
    simulation::HeadlessWorld,
};

#[test]
fn test_authenticated_handshake_and_session_token_enforcement() {
    let auth_key = "test-secret-key-12345";
    let mut server = DedicatedServer::with_world("127.0.0.1:0", HeadlessWorld::new().unwrap())
        .unwrap()
        .with_auth(auth_key);
    let server_addr = server.local_addr;

    let mut client = UdpTransport::bind("127.0.0.1:0").unwrap();
    let content_hash = server.world.content_hash;

    // 1. Initial Hello
    client
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                content_hash,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();

    server.poll_network().unwrap();

    // 2. Client receives AuthChallenge
    let (challenge_pkt, from) = client.recv_packet().unwrap().expect("AuthChallenge packet");
    assert_eq!(from, server_addr);
    let (nonce, salt) = match challenge_pkt {
        Packet::AuthChallenge { nonce, salt } => (nonce, salt),
        other => panic!("Expected AuthChallenge, got {:?}", other),
    };

    // 3. Negative test: Attacker on different socket attempts invalid proof
    let mut attacker = UdpTransport::bind("127.0.0.1:0").unwrap();
    attacker
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                content_hash,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    let (att_chal, _) = attacker.recv_packet().unwrap().expect("Attacker challenge");
    let (att_nonce, _att_salt) = match att_chal {
        Packet::AuthChallenge { nonce, salt } => (nonce, salt),
        other => panic!("Expected AuthChallenge for attacker, got {:?}", other),
    };
    let bad_proof = [0u8; 32];
    attacker
        .send_packet(
            &Packet::AuthResponse {
                player_id: 0,
                nonce: att_nonce,
                proof: bad_proof,
                content_hash,
            },
            server_addr,
        )
        .unwrap();

    server.poll_network().unwrap();
    // Server must reject invalid proof: no session created
    assert_eq!(server.sessions.len(), 0);

    // 4. Positive test: Legitimate client computes correct HMAC-SHA256 proof for its challenge
    let valid_proof = compute_auth_proof(auth_key, nonce, 0, &salt);
    client
        .send_packet(
            &Packet::AuthResponse {
                player_id: 0,
                nonce,
                proof: valid_proof,
                content_hash,
            },
            server_addr,
        )
        .unwrap();

    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 1, "Session must be created");

    // Client receives Welcome with assigned player ID and session token
    let (welcome_pkt, _) = client.recv_packet().unwrap().expect("Welcome packet");
    let (assigned_id, session_token) = match welcome_pkt {
        Packet::Welcome {
            player_id,
            session_token,
            ..
        } => (player_id, session_token.expect("session token in Welcome")),
        other => panic!("Expected Welcome, got {:?}", other),
    };
    assert_eq!(assigned_id, 1);
    assert_eq!(
        server
            .session_registry
            .get_by_peer(&client.local_addr().unwrap())
            .expect("authoritative registry entry")
            .token,
        session_token,
        "Welcome and registry must use the same session token"
    );

    // 5. Input enforcement: unauthorized input with bogus session token must be dropped
    let bogus_token = [9999, 8888];
    let bad_input = InputFrame {
        client_tick: 1,
        movement: Movement {
            forward: 1.0,
            ..Default::default()
        },
        yaw: 0.0,
        pitch: 0.0,
        fire_wrench: false,
        fire_pistol: false,
        interact: false,
        ack_server_tick: 0,
        session_token: Some(bogus_token),
    };
    client
        .send_packet(&Packet::Input(bad_input), server_addr)
        .unwrap();
    server.poll_network().unwrap();
    // Player position remains unchanged at spawn
    assert_eq!(
        server.world.player(assigned_id).unwrap().position,
        vesper3d::viewer::test_lab::SPAWN_PLAYER_1
    );

    // 6. Authorized input with valid session token is accepted and simulated
    let valid_input = InputFrame {
        client_tick: 2,
        movement: Movement {
            forward: 1.0,
            ..Default::default()
        },
        yaw: 0.0,
        pitch: 0.0,
        fire_wrench: false,
        fire_pistol: false,
        interact: false,
        ack_server_tick: 0,
        session_token: Some(session_token),
    };
    client
        .send_packet(&Packet::Input(valid_input), server_addr)
        .unwrap();
    server.poll_network().unwrap();
    server.world.step();
    // Player moved forward
    assert!(
        server.world.player(assigned_id).unwrap().position.2
            < vesper3d::viewer::test_lab::SPAWN_PLAYER_1.2
    );

    // 7. A keyed session requires a credential; omission is not accepted.
    client
        .send_packet(
            &Packet::Disconnect {
                player_id: assigned_id,
                session_token: None,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(
        server.sessions.len(),
        1,
        "Session survives disconnect with omitted credentials"
    );

    // 8. Anti-spoofing disconnect: mismatched token rejected
    client
        .send_packet(
            &Packet::Disconnect {
                player_id: assigned_id,
                session_token: Some(bogus_token),
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(
        server.sessions.len(),
        1,
        "Session survives spoofed disconnect"
    );

    // 9. Graceful disconnect with matching session token
    client
        .send_packet(
            &Packet::Disconnect {
                player_id: assigned_id,
                session_token: Some(session_token),
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 0, "Session cleanly removed");
    assert_eq!(
        server.session_registry.count(),
        0,
        "Registry entry is removed with the client session"
    );
}

#[test]
fn test_live_udp_proxy_forwarding() {
    let mut server_sock = UdpTransport::bind("127.0.0.1:0").unwrap();
    let server_addr = server_sock.local_addr().unwrap();

    let proxy_config = NetworkProxyConfig::clean_delay(5.0); // 5ms delay, 0 loss
    let mut proxy = UdpProxyServer::bind("127.0.0.1:0", server_addr, proxy_config).unwrap();
    let proxy_addr = proxy.local_addr().unwrap();

    let mut client = UdpTransport::bind("127.0.0.1:0").unwrap();

    // Client sends packet to proxy
    let hello = Packet::Hello {
        protocol_version: PROTOCOL_VERSION,
        content_hash: 1337,
        player_id: 0,
    };
    client.send_packet(&hello, proxy_addr).unwrap();

    let start = Instant::now();
    let mut forwarded_to_server = false;

    // Pump proxy for up to 50ms
    while start.elapsed() < Duration::from_millis(50) {
        proxy.poll(Instant::now()).unwrap();
        if let Ok(Some((pkt, src))) = server_sock.recv_packet() {
            match pkt {
                Packet::Hello {
                    protocol_version,
                    content_hash,
                    ..
                } => {
                    assert_eq!(protocol_version, PROTOCOL_VERSION);
                    assert_eq!(content_hash, 1337);
                }
                other => panic!("Expected Hello, got {:?}", other),
            }
            // Server replies through proxy
            let welcome = Packet::Welcome {
                player_id: 1,
                server_tick: 42,
                map_name: "Test Lab".into(),
                session_token: None,
            };
            server_sock.send_packet(&welcome, src).unwrap();
            forwarded_to_server = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(forwarded_to_server, "Proxy forwarded upstream packet");

    // Pump proxy to deliver downstream reply to client
    let mut forwarded_to_client = false;
    let reply_start = Instant::now();
    while reply_start.elapsed() < Duration::from_millis(50) {
        proxy.poll(Instant::now()).unwrap();
        if let Ok(Some((pkt, _))) = client.recv_packet() {
            match pkt {
                Packet::Welcome { player_id, .. } => {
                    assert_eq!(player_id, 1);
                    forwarded_to_client = true;
                    break;
                }
                other => panic!("Unexpected downstream packet {:?}", other),
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(forwarded_to_client, "Proxy forwarded downstream packet");
}
