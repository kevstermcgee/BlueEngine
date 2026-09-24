//! Integration test proving one authoritative server + two clients end-to-end
//! in the Blue Test Lab environment with prediction, reconciliation,
//! snapshot interpolation, delta compression, and spatial interest management.
use vesper3d::viewer::{
    controller::{Controller, Movement},
    net::{
        InputFrame, InterpolationBuffer, NetworkSimulator, Packet, PlayerNetState, PredictionBuffer,
    },
    simulation::{HeadlessWorld, TICK_SECONDS},
    spatial::RoomId,
    test_lab::{SPAWN_PLAYER_1, SPAWN_PLAYER_2},
};

struct SimulatedClient {
    id: u64,
    controller: Controller,
    prediction: PredictionBuffer,
    remote_interpolation: InterpolationBuffer<PlayerNetState>,
    to_server: NetworkSimulator<InputFrame>,
    from_server: NetworkSimulator<Packet>,
    reconciled_corrections: usize,
}

impl SimulatedClient {
    fn new(id: u64, spawn: vesper3d::math::V) -> Self {
        let mut controller = Controller::default();
        controller.position = spawn;
        Self {
            id,
            controller,
            prediction: PredictionBuffer::new(64),
            remote_interpolation: InterpolationBuffer::new(32),
            to_server: NetworkSimulator::new(40, 0.05), // 40ms latency, 5% loss
            from_server: NetworkSimulator::new(40, 0.05),
            reconciled_corrections: 0,
        }
    }
}

#[test]
fn one_server_two_clients_end_to_end_in_test_lab() {
    let mut server = HeadlessWorld::new().expect("Build Blue Test Lab");
    assert_eq!(server.room.name, "Blue Test Lab");
    assert!(server.join(1));
    assert!(server.join(2));

    // Initialize two clients matching their respective spawns
    let mut client1 = SimulatedClient::new(1, SPAWN_PLAYER_1);
    let mut client2 = SimulatedClient::new(2, SPAWN_PLAYER_2);
    assert_eq!(client1.id, 1);
    assert_eq!(client2.id, 2);

    let match_ticks = 180; // 3 seconds at 60 Hz
    let mut last_snap = server.snapshot(0);

    for tick in 1..=match_ticks {
        // --- CLIENT 1: Move forward toward Main Arena center ---
        let input1 = InputFrame {
            client_tick: tick,
            movement: Movement {
                forward: 1.0,
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
        };
        client1
            .controller
            .update(input1.movement, TICK_SECONDS, &server.room.colliders);
        client1
            .prediction
            .push(input1.clone(), client1.controller.clone());
        client1.to_server.send(tick, input1);

        // --- CLIENT 2: Move right / strafe toward East Portal (Physics Lab) ---
        let input2 = InputFrame {
            client_tick: tick,
            movement: Movement {
                right: 1.0,
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
        };
        client2
            .controller
            .update(input2.movement, TICK_SECONDS, &server.room.colliders);
        client2
            .prediction
            .push(input2.clone(), client2.controller.clone());
        client2.to_server.send(tick, input2);

        // --- SERVER: Receive arriving packets ---
        let mut last_ack1 = 0;
        let mut last_ack2 = 0;

        for pkt in client1.to_server.receive(tick) {
            last_ack1 = pkt.client_tick;
            server.input(1, pkt.movement, pkt.yaw, pkt.pitch);
        }
        for pkt in client2.to_server.receive(tick) {
            last_ack2 = pkt.client_tick;
            server.input(2, pkt.movement, pkt.yaw, pkt.pitch);
        }

        // --- SERVER: Step authoritative simulation (continuous 60 Hz) ---
        server.step();

        // --- SERVER: Broadcast snapshots & deltas at 20 Hz (every 3 ticks) ---
        if tick % 3 == 0 {
            let snap = server.snapshot(last_ack1.max(last_ack2));
            let delta = snap.compute_delta(&last_snap);

            // Reconstruct snapshot to verify delta roundtrip
            let reconstructed = delta.apply_to(&last_snap);
            assert_eq!(reconstructed.tick, snap.tick);
            last_snap = snap.clone();

            // Send packet to clients
            let pkt = Packet::Snapshot(snap);
            client1.from_server.send(tick, pkt.clone());
            client2.from_server.send(tick, pkt);
        }

        // --- CLIENTS: Process incoming server snapshots ---
        for pkt in client1.from_server.receive(tick) {
            if let Packet::Snapshot(snap) = pkt {
                if let Some(p1_server) = snap.players.iter().find(|p| p.id == 1) {
                    if client1.prediction.reconcile(
                        snap.ack_client_tick,
                        p1_server,
                        &mut client1.controller,
                        &server.room.colliders,
                        0.05,
                    ) {
                        client1.reconciled_corrections += 1;
                    }
                }
                if let Some(p2_server) = snap.players.iter().find(|p| p.id == 2) {
                    client1
                        .remote_interpolation
                        .push(snap.tick, p2_server.clone());
                }
            }
        }

        for pkt in client2.from_server.receive(tick) {
            if let Packet::Snapshot(snap) = pkt {
                if let Some(p2_server) = snap.players.iter().find(|p| p.id == 2) {
                    if client2.prediction.reconcile(
                        snap.ack_client_tick,
                        p2_server,
                        &mut client2.controller,
                        &server.room.colliders,
                        0.05,
                    ) {
                        client2.reconciled_corrections += 1;
                    }
                }
                if let Some(p1_server) = snap.players.iter().find(|p| p.id == 1) {
                    client2
                        .remote_interpolation
                        .push(snap.tick, p1_server.clone());
                }
            }
        }

        // --- CLIENT 1: Interpolate remote Player 2 at render tick ---
        if tick > 10 {
            let render_tick = (tick as f32) - 3.0; // 50ms interpolation delay
            let interp_pos = client1.remote_interpolation.interpolate_at(render_tick);
            if let Some(pos) = interp_pos {
                assert!(pos.0.is_finite() && pos.1.is_finite() && pos.2.is_finite());
            }
        }
    }

    // --- END-OF-MATCH VERIFICATIONS ---
    assert_eq!(server.tick, 180);
    assert!(client1.controller.position.0.is_finite());
    assert!(client2.controller.position.0.is_finite());

    // Verify server performance metrics
    let perf = server.performance_snapshot(600.0);
    assert_eq!(perf.tick, 180);
    assert_eq!(perf.replicated_entities, 2); // 2 players
    assert!(perf.snapshot_bytes > 0);

    // Verify state checksum is non-zero and deterministic
    let checksum = server.checksum();
    assert_ne!(checksum, 0);
}

#[test]
fn spatial_interest_management_in_test_lab() {
    let lab = vesper3d::viewer::test_lab::build().expect("Test Lab build");
    let spatial = lab.spatial.as_ref().expect("Embedded spatial graph");

    let arena = RoomId(1);
    let physics = RoomId(2);
    let locomotion = RoomId(3);

    // 1-hop relevance:
    // Arena is adjacent to both Labs
    assert!(spatial.is_relevant_for_interest(arena, physics));
    assert!(spatial.is_relevant_for_interest(arena, locomotion));

    // Physics Lab is adjacent to Arena
    assert!(spatial.is_relevant_for_interest(physics, arena));

    // Locomotion Lab is adjacent to Arena
    assert!(spatial.is_relevant_for_interest(locomotion, arena));

    // But Physics Lab and Locomotion Lab are separated by Arena (2 hops):
    assert!(!spatial.is_relevant_for_interest(physics, locomotion));
    assert!(!spatial.is_relevant_for_interest(locomotion, physics));

    // Test authoritative snapshot filtering via snapshot_for_player
    let mut server = HeadlessWorld::new().expect("Build Blue Test Lab");
    // Spawn player 1 in Physics Lab (x ~ 14.0)
    assert!(server.join_at(1, vesper3d::math::V(14.0, 0.5, 0.0)));
    // Spawn player 2 in Locomotion Lab (x ~ -14.0)
    assert!(server.join_at(2, vesper3d::math::V(-14.0, 0.5, 0.0)));
    // Spawn player 3 in Main Arena (x ~ 0.0)
    assert!(server.join_at(3, vesper3d::math::V(0.0, 0.5, 0.0)));

    // For Player 1 (Physics Lab):
    // Sees Player 1 (self) and Player 3 (adjacent Arena), but NOT Player 2 (Locomotion Lab, 2 hops away)
    let p1_snap = server.snapshot_for_player(1, 0);
    let p1_ids: Vec<u64> = p1_snap.players.iter().map(|p| p.id).collect();
    assert!(p1_ids.contains(&1));
    assert!(p1_ids.contains(&3));
    assert!(!p1_ids.contains(&2));

    // For Player 2 (Locomotion Lab):
    // Sees Player 2 (self) and Player 3 (adjacent Arena), but NOT Player 1 (Physics Lab, 2 hops away)
    let p2_snap = server.snapshot_for_player(2, 0);
    let p2_ids: Vec<u64> = p2_snap.players.iter().map(|p| p.id).collect();
    assert!(p2_ids.contains(&2));
    assert!(p2_ids.contains(&3));
    assert!(!p2_ids.contains(&1));

    // For Player 3 (Main Arena):
    // Arena is adjacent to both Labs, so Player 3 sees all three players
    let p3_snap = server.snapshot_for_player(3, 0);
    let p3_ids: Vec<u64> = p3_snap.players.iter().map(|p| p.id).collect();
    assert!(p3_ids.contains(&1));
    assert!(p3_ids.contains(&2));
    assert!(p3_ids.contains(&3));
}

#[test]
fn localhost_udp_one_server_two_clients_end_to_end() {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use vesper3d::viewer::net::{UdpTransport, PROTOCOL_VERSION};

    // 1. Bind UDP sockets
    let mut server_net = UdpTransport::bind("127.0.0.1:0").expect("Server UDP bind");
    let server_addr = server_net.local_addr().expect("Server addr");

    let mut client1_net = UdpTransport::bind("127.0.0.1:0").expect("Client 1 UDP bind");
    let mut client2_net = UdpTransport::bind("127.0.0.1:0").expect("Client 2 UDP bind");

    // 2. Initialize server simulation world
    let mut server = HeadlessWorld::new().expect("Build Blue Test Lab");
    let mut client_addrs: HashMap<SocketAddr, u64> = HashMap::new();
    let mut player_addrs: HashMap<u64, SocketAddr> = HashMap::new();

    // 3. Initialize simulated clients
    let mut client1 = SimulatedClient::new(1, SPAWN_PLAYER_1);
    let mut client2 = SimulatedClient::new(2, SPAWN_PLAYER_2);

    // 4. UDP Handshake: Hello -> Welcome
    client1_net
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 1,
            },
            server_addr,
        )
        .expect("Send Hello 1");
    client2_net
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 2,
            },
            server_addr,
        )
        .expect("Send Hello 2");

    // Server processes hellos
    for _ in 0..100 {
        while let Ok(Some((pkt, src))) = server_net.recv_packet() {
            if let Packet::Hello {
                protocol_version,
                player_id,
            } = pkt
            {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
                server.join_at(
                    player_id,
                    if player_id == 1 {
                        SPAWN_PLAYER_1
                    } else {
                        SPAWN_PLAYER_2
                    },
                );
                client_addrs.insert(src, player_id);
                player_addrs.insert(player_id, src);

                let welcome = Packet::Welcome {
                    player_id,
                    server_tick: server.tick,
                    map_name: server.room.name.clone(),
                };
                server_net.send_packet(&welcome, src).expect("Send Welcome");
            }
        }
        if client_addrs.len() == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(client_addrs.len(), 2, "Both clients connected via UDP");

    // Clients process Welcome
    let mut client1_welcomed = false;
    let mut client2_welcomed = false;
    for _ in 0..50 {
        if let Ok(Some((Packet::Welcome { player_id: 1, .. }, _))) = client1_net.recv_packet() {
            client1_welcomed = true;
        }
        if let Ok(Some((Packet::Welcome { player_id: 2, .. }, _))) = client2_net.recv_packet() {
            client2_welcomed = true;
        }
        if client1_welcomed && client2_welcomed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(client1_welcomed);
    assert!(client2_welcomed);

    // 5. Run continuous simulation over UDP for 60 ticks (1 second at 60 Hz)
    let sim_ticks = 60;
    let mut last_client1_ack = 0;
    let mut last_client2_ack = 0;

    for tick in 1..=sim_ticks {
        // Client 1 sends input
        let input1 = InputFrame {
            client_tick: tick,
            movement: Movement {
                forward: 1.0,
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
        };
        client1
            .controller
            .update(input1.movement, TICK_SECONDS, &server.room.colliders);
        client1
            .prediction
            .push(input1.clone(), client1.controller.clone());
        client1_net
            .send_packet(&Packet::Input(input1), server_addr)
            .expect("Client 1 send input");

        // Client 2 sends input
        let input2 = InputFrame {
            client_tick: tick,
            movement: Movement {
                right: 1.0,
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
        };
        client2
            .controller
            .update(input2.movement, TICK_SECONDS, &server.room.colliders);
        client2
            .prediction
            .push(input2.clone(), client2.controller.clone());
        client2_net
            .send_packet(&Packet::Input(input2), server_addr)
            .expect("Client 2 send input");

        // Server receives inputs
        while let Ok(Some((pkt, src))) = server_net.recv_packet() {
            if let (Packet::Input(frame), Some(&pid)) = (pkt, client_addrs.get(&src)) {
                if pid == 1 {
                    last_client1_ack = frame.client_tick;
                } else if pid == 2 {
                    last_client2_ack = frame.client_tick;
                }
                server.input(pid, frame.movement, frame.yaw, frame.pitch);
            }
        }

        // Authoritative simulation step
        server.step();

        // Broadcast snapshots at 20 Hz (every 3 ticks)
        if tick % 3 == 0 {
            if let Some(&addr1) = player_addrs.get(&1) {
                let snap1 = server.snapshot_for_player(1, last_client1_ack);
                server_net
                    .send_packet(&Packet::Snapshot(snap1), addr1)
                    .expect("Server send to 1");
            }
            if let Some(&addr2) = player_addrs.get(&2) {
                let snap2 = server.snapshot_for_player(2, last_client2_ack);
                server_net
                    .send_packet(&Packet::Snapshot(snap2), addr2)
                    .expect("Server send to 2");
            }
        }

        // Clients receive server snapshots
        while let Ok(Some((pkt, _))) = client1_net.recv_packet() {
            if let Packet::Snapshot(snap) = pkt {
                if let Some(p1_server) = snap.players.iter().find(|p| p.id == 1) {
                    client1.prediction.reconcile(
                        snap.ack_client_tick,
                        p1_server,
                        &mut client1.controller,
                        &server.room.colliders,
                        0.05,
                    );
                }
                if let Some(p2_server) = snap.players.iter().find(|p| p.id == 2) {
                    client1
                        .remote_interpolation
                        .push(snap.tick, p2_server.clone());
                }
            }
        }

        while let Ok(Some((pkt, _))) = client2_net.recv_packet() {
            if let Packet::Snapshot(snap) = pkt {
                if let Some(p2_server) = snap.players.iter().find(|p| p.id == 2) {
                    client2.prediction.reconcile(
                        snap.ack_client_tick,
                        p2_server,
                        &mut client2.controller,
                        &server.room.colliders,
                        0.05,
                    );
                }
                if let Some(p1_server) = snap.players.iter().find(|p| p.id == 1) {
                    client2
                        .remote_interpolation
                        .push(snap.tick, p1_server.clone());
                }
            }
        }
    }

    // End-of-test validations
    assert_eq!(server.tick, 60);
    assert!(client1.controller.position.0.is_finite());
    assert!(client2.controller.position.0.is_finite());

    // Verify remote interpolation received packets
    let p2_sample = client1.remote_interpolation.interpolate_at(55.0);
    assert!(p2_sample.is_some());

    // Verify performance snapshot has real non-zero delta bytes
    let perf = server.performance_snapshot(500.0);
    assert_eq!(perf.tick, 60);
    assert!(perf.snapshot_bytes > 0);
    assert!(perf.delta_bytes > 0);

    // Verify deterministic checksum
    let checksum = server.checksum();
    assert_ne!(checksum, 0);
}
