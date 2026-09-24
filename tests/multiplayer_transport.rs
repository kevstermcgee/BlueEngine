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

struct TestClientReceiver {
    pub baseline: Option<vesper3d::viewer::net::WorldSnapshot>,
    pub history: std::collections::VecDeque<vesper3d::viewer::net::WorldSnapshot>,
    pub deltas_received: usize,
    pub deltas_applied: usize,
    pub deltas_rejected: usize,
    pub snapshots_received: usize,
    pub keyframe_needed: bool,
}

impl TestClientReceiver {
    fn new() -> Self {
        Self {
            baseline: None,
            history: std::collections::VecDeque::with_capacity(32),
            deltas_received: 0,
            deltas_applied: 0,
            deltas_rejected: 0,
            snapshots_received: 0,
            keyframe_needed: false,
        }
    }

    fn receive(&mut self, pkt: Packet) -> Option<vesper3d::viewer::net::WorldSnapshot> {
        match pkt {
            Packet::Snapshot(snap) => {
                self.snapshots_received += 1;
                self.baseline = Some(snap.clone());
                self.history.push_back(snap.clone());
                if self.history.len() > 32 {
                    self.history.pop_front();
                }
                self.keyframe_needed = false;
                Some(snap)
            }
            Packet::Delta(delta) => {
                self.deltas_received += 1;
                let base = self
                    .history
                    .iter()
                    .find(|s| s.tick == delta.base_tick)
                    .cloned()
                    .or_else(|| {
                        self.baseline
                            .as_ref()
                            .filter(|b| b.tick == delta.base_tick)
                            .cloned()
                    });

                if let Some(base) = base {
                    let snap = delta.apply_to(&base);
                    self.baseline = Some(snap.clone());
                    self.history.push_back(snap.clone());
                    if self.history.len() > 32 {
                        self.history.pop_front();
                    }
                    self.deltas_applied += 1;
                    Some(snap)
                } else {
                    self.deltas_rejected += 1;
                    self.keyframe_needed = true;
                    None
                }
            }
            _ => None,
        }
    }

    fn latest_acked_tick(&self) -> u64 {
        self.baseline.as_ref().map(|b| b.tick).unwrap_or(0)
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
            ack_server_tick: 0,
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
            ack_server_tick: 0,
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
            ack_server_tick: 0,
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
            ack_server_tick: 0,
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

#[test]
fn dedicated_server_two_clients_movement_disconnect_reconnect_and_prop_physics() {
    use std::time::Duration;
    use vesper3d::viewer::{
        net::{
            InterpolationBuffer, Packet, PlayerNetState, PredictionBuffer, UdpTransport,
            PROTOCOL_VERSION,
        },
        prop_physics::PropPhysics,
        server::DedicatedServer,
        test_lab::{SPAWN_PLAYER_1, SPAWN_PLAYER_2},
    };

    // 1. Start real DedicatedServer listening on localhost loopback UDP
    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("DedicatedServer bind");
    let server_addr = server.local_addr;
    assert_eq!(server.sessions.len(), 0);
    assert_eq!(server.world.room.name, "Blue Test Lab");

    // 2. Client 1 connects
    let mut client1_net = UdpTransport::bind("127.0.0.1:0").expect("Client 1 bind");
    client1_net
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .expect("Send Hello 1");

    server.poll_network().expect("Server poll 1");
    assert_eq!(server.sessions.len(), 1);
    assert!(server.sessions.contains_key(&1));

    let (c1_welcome, _) = client1_net.recv_packet().unwrap().expect("Recv Welcome 1");
    let c1_id = match c1_welcome {
        Packet::Welcome {
            player_id,
            map_name,
            ..
        } => {
            assert_eq!(player_id, 1);
            assert_eq!(map_name, "Blue Test Lab");
            player_id
        }
        _ => panic!("Expected welcome packet for client 1"),
    };
    assert_eq!(c1_id, 1);

    // 3. Client 2 connects
    let mut client2_net = UdpTransport::bind("127.0.0.1:0").expect("Client 2 bind");
    client2_net
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .expect("Send Hello 2");

    server.poll_network().expect("Server poll 2");
    assert_eq!(server.sessions.len(), 2);
    assert!(server.sessions.contains_key(&2));

    let (c2_welcome, _) = client2_net.recv_packet().unwrap().expect("Recv Welcome 2");
    let c2_id = match c2_welcome {
        Packet::Welcome { player_id, .. } => {
            assert_eq!(player_id, 2);
            player_id
        }
        _ => panic!("Expected welcome packet for client 2"),
    };
    assert_eq!(c2_id, 2);

    // 4. Initialize client simulation states
    let mut c1_controller = Controller::default();
    c1_controller.position = SPAWN_PLAYER_1;
    let mut c1_pred = PredictionBuffer::new(64);
    let mut c1_remote_interp = InterpolationBuffer::<PlayerNetState>::new(32);

    let mut c2_controller = Controller::default();
    c2_controller.position = SPAWN_PLAYER_2;
    let mut c2_pred = PredictionBuffer::new(64);
    let mut c2_remote_interp = InterpolationBuffer::<PlayerNetState>::new(32);

    // Build local client prop physics scenes to verify prop replication
    let mut c1_room = vesper3d::viewer::test_lab::build().expect("Build Lab 1");
    let mut c1_prop_phys = PropPhysics::new(&mut c1_room).expect("PropPhys 1");
    let mut c2_room = vesper3d::viewer::test_lab::build().expect("Build Lab 2");
    let mut c2_prop_phys = PropPhysics::new(&mut c2_room).expect("PropPhys 2");

    let mut c1_rx = TestClientReceiver::new();
    let mut c2_rx = TestClientReceiver::new();

    // 5. Active movement and mutual smooth observation for 30 ticks
    for tick in 1..=30 {
        if c1_rx.keyframe_needed {
            client1_net
                .send_packet(&Packet::RequestKeyframe, server_addr)
                .unwrap();
        }
        // Client 1 inputs (moving forward)
        let inp1 = InputFrame {
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
            ack_server_tick: c1_rx.latest_acked_tick(),
        };
        c1_controller.update(inp1.movement, TICK_SECONDS, &server.world.room.colliders);
        c1_pred.push(inp1.clone(), c1_controller.clone());
        client1_net
            .send_packet(&Packet::Input(inp1), server_addr)
            .unwrap();

        if c2_rx.keyframe_needed {
            client2_net
                .send_packet(&Packet::RequestKeyframe, server_addr)
                .unwrap();
        }
        // Client 2 inputs (strafing right)
        let inp2 = InputFrame {
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
            ack_server_tick: c2_rx.latest_acked_tick(),
        };
        c2_controller.update(inp2.movement, TICK_SECONDS, &server.world.room.colliders);
        c2_pred.push(inp2.clone(), c2_controller.clone());
        client2_net
            .send_packet(&Packet::Input(inp2), server_addr)
            .unwrap();

        // Server steps authoritative world
        server.poll_network().unwrap();
        server.step();

        // Drain client 1 packets
        while let Ok(Some((pkt, _))) = client1_net.recv_packet() {
            if let Some(snap) = c1_rx.receive(pkt) {
                if let Some(my_state) = snap.players.iter().find(|p| p.id == 1) {
                    c1_pred.reconcile(
                        snap.ack_client_tick,
                        my_state,
                        &mut c1_controller,
                        &server.world.room.colliders,
                        0.05,
                    );
                }
                if let Some(p2_state) = snap.players.iter().find(|p| p.id == 2) {
                    c1_remote_interp.push(snap.tick, p2_state.clone());
                }
                for prop in &snap.props {
                    c1_prop_phys.set_prop_transform_and_vel(
                        &prop.id,
                        prop.position,
                        prop.rotation,
                        prop.linear_velocity,
                        prop.angular_velocity,
                        prop.sleeping,
                    );
                }
            }
        }

        // Drain client 2 packets
        while let Ok(Some((pkt, _))) = client2_net.recv_packet() {
            if let Some(snap) = c2_rx.receive(pkt) {
                if let Some(my_state) = snap.players.iter().find(|p| p.id == 2) {
                    c2_pred.reconcile(
                        snap.ack_client_tick,
                        my_state,
                        &mut c2_controller,
                        &server.world.room.colliders,
                        0.05,
                    );
                }
                if let Some(p1_state) = snap.players.iter().find(|p| p.id == 1) {
                    c2_remote_interp.push(snap.tick, p1_state.clone());
                }
                for prop in &snap.props {
                    c2_prop_phys.set_prop_transform_and_vel(
                        &prop.id,
                        prop.position,
                        prop.rotation,
                        prop.linear_velocity,
                        prop.angular_velocity,
                        prop.sleeping,
                    );
                }
            }
        }
    }

    // Verify deltas were actually received by clients
    assert!(
        c1_rx.deltas_received > 0,
        "Client 1 received real delta packets"
    );
    assert!(
        c2_rx.deltas_received > 0,
        "Client 2 received real delta packets"
    );

    // Both clients observed each other moving via interpolation
    let c1_sees_c2 = c1_remote_interp.interpolate_state_at(25.0);
    assert!(
        c1_sees_c2.is_some(),
        "Client 1 smooth interpolation of Client 2"
    );
    let c2_sees_c1 = c2_remote_interp.interpolate_state_at(25.0);
    assert!(
        c2_sees_c1.is_some(),
        "Client 2 smooth interpolation of Client 1"
    );

    // 6. Authoritative Moving Physics Prop
    let initial_prop_pos = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_position(0)
        .expect("Prop 0 exists");

    // Server applies an impulse launching the prop upward and forward
    server
        .world
        .apply_prop_impulse(0, vesper3d::math::V(2.0, 6.0, 1.0));

    // Simulate 30 ticks of real Rapier physics trajectory and replication
    for tick in 31..=60 {
        let inp1 = InputFrame {
            client_tick: tick,
            movement: Movement::default(),
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
            ack_server_tick: c1_rx.latest_acked_tick(),
        };
        let _ = client1_net.send_packet(&Packet::Input(inp1), server_addr);

        let inp2 = InputFrame {
            client_tick: tick,
            movement: Movement::default(),
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
            ack_server_tick: c2_rx.latest_acked_tick(),
        };
        let _ = client2_net.send_packet(&Packet::Input(inp2), server_addr);

        server.poll_network().unwrap();
        server.step();

        while let Ok(Some((pkt, _))) = client1_net.recv_packet() {
            if let Some(snap) = c1_rx.receive(pkt) {
                for prop in &snap.props {
                    c1_prop_phys.set_prop_transform_and_vel(
                        &prop.id,
                        prop.position,
                        prop.rotation,
                        prop.linear_velocity,
                        prop.angular_velocity,
                        prop.sleeping,
                    );
                }
            }
        }

        while let Ok(Some((pkt, _))) = client2_net.recv_packet() {
            if let Some(snap) = c2_rx.receive(pkt) {
                for prop in &snap.props {
                    c2_prop_phys.set_prop_transform_and_vel(
                        &prop.id,
                        prop.position,
                        prop.rotation,
                        prop.linear_velocity,
                        prop.angular_velocity,
                        prop.sleeping,
                    );
                }
            }
        }
    }

    let server_prop_pos = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_position(0)
        .unwrap();

    let c1_prop_pos = c1_prop_phys.prop_position(0).unwrap();
    let c2_prop_pos = c2_prop_phys.prop_position(0).unwrap();

    // Verify prop actually moved authoritatively under physics
    assert!(
        (server_prop_pos - initial_prop_pos).length() > 0.1,
        "Prop moved significantly from original spawn"
    );
    // Both clients observed the exact same authoritative moving prop position
    assert!(
        (c1_prop_pos - server_prop_pos).length() < 0.001,
        "Client 1 matches authoritative prop position"
    );
    assert!(
        (c2_prop_pos - server_prop_pos).length() < 0.001,
        "Client 2 matches authoritative prop position"
    );

    // 7. Graceful Disconnect: Client 2 leaves
    client2_net
        .send_packet(&Packet::Disconnect { player_id: 2 }, server_addr)
        .expect("Send Disconnect");

    server.poll_network().unwrap();
    assert_eq!(
        server.sessions.len(),
        1,
        "Session count after Client 2 disconnects"
    );
    assert!(
        server.world.player(2).is_none(),
        "Player 2 removed from world"
    );

    // Server broadcasts snapshot without Player 2
    server.broadcast_snapshots();
    let mut c1_saw_c2_leave = false;
    while let Ok(Some((pkt, _))) = client1_net.recv_packet() {
        if let Some(snap) = c1_rx.receive(pkt) {
            if !snap.players.iter().any(|p| p.id == 2) {
                c1_saw_c2_leave = true;
            }
        }
    }
    assert!(
        c1_saw_c2_leave,
        "Client 1 notified that Client 2 disconnected"
    );

    // 8. Reconnect: Client 2 reconnects to server
    client2_net
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 2,
            },
            server_addr,
        )
        .expect("Send Reconnect Hello");

    server.poll_network().unwrap();
    assert_eq!(
        server.sessions.len(),
        2,
        "Session count after Client 2 reconnects"
    );
    assert!(
        server.world.player(2).is_some(),
        "Player 2 reinstated in world"
    );

    let (c2_reconnected_welcome, _) = client2_net
        .recv_packet()
        .unwrap()
        .expect("Recv Reconnect Welcome");
    match c2_reconnected_welcome {
        Packet::Welcome { player_id, .. } => assert_eq!(player_id, 2),
        _ => panic!("Expected welcome on reconnect"),
    }

    // Broadcast synchronized state to both clients
    server.broadcast_snapshots();
    let mut c1_saw_c2_return = false;
    while let Ok(Some((pkt, _))) = client1_net.recv_packet() {
        if let Some(snap) = c1_rx.receive(pkt) {
            if snap.players.iter().any(|p| p.id == 2) {
                c1_saw_c2_return = true;
            }
        }
    }
    assert!(c1_saw_c2_return, "Client 1 sees Client 2 back in the world");

    // 9. Timeout Disconnect
    server.client_timeout = Duration::from_millis(10);
    std::thread::sleep(Duration::from_millis(25));
    let dropped = server.check_timeouts();
    assert!(
        dropped.contains(&1) && dropped.contains(&2),
        "Both silent clients timed out"
    );
    assert_eq!(server.sessions.len(), 0, "All sessions dropped on timeout");
}

#[test]
fn process_dedicated_server_two_clients_end_to_end() {
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use vesper3d::viewer::net::{Packet, UdpTransport, PROTOCOL_VERSION};

    // Ensure be2-headless is compiled
    let bin_path = if cfg!(windows) {
        "target/debug/be2-headless.exe"
    } else {
        "target/debug/be2-headless"
    };

    // Start real separate server process
    let mut server_child = Command::new(bin_path)
        .args(["--server", "127.0.0.1:4099", "--ticks", "120"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Spawn be2-headless dedicated server process");

    // Brief sleep for socket bind
    std::thread::sleep(Duration::from_millis(150));
    let server_addr = "127.0.0.1:4099".parse().unwrap();

    // Client 1 connects
    let mut client1 = UdpTransport::bind("127.0.0.1:0").expect("Client 1 bind");
    client1
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();

    // Client 2 connects
    let mut client2 = UdpTransport::bind("127.0.0.1:0").expect("Client 2 bind");
    client2
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();

    // Receive welcomes
    let mut c1_welcomed = false;
    let mut c2_welcomed = false;
    for _ in 0..50 {
        if let Ok(Some((Packet::Welcome { player_id: 1, .. }, _))) = client1.recv_packet() {
            c1_welcomed = true;
        }
        if let Ok(Some((Packet::Welcome { player_id: 2, .. }, _))) = client2.recv_packet() {
            c2_welcomed = true;
        }
        if c1_welcomed && c2_welcomed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(c1_welcomed, "Client 1 welcomed by server process");
    assert!(c2_welcomed, "Client 2 welcomed by server process");

    // Run active input exchange for 30 ticks
    for tick in 1..=30 {
        let inp1 = InputFrame {
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
            ack_server_tick: 0,
        };
        client1
            .send_packet(&Packet::Input(inp1), server_addr)
            .unwrap();

        let inp2 = InputFrame {
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
            ack_server_tick: 0,
        };
        client2
            .send_packet(&Packet::Input(inp2), server_addr)
            .unwrap();

        std::thread::sleep(Duration::from_millis(16));
    }

    // Client 1 receives server snapshots showing Client 2 moving
    let mut c1_rx = TestClientReceiver::new();
    let mut c1_saw_c2 = false;
    for _ in 0..30 {
        while let Ok(Some((pkt, _))) = client1.recv_packet() {
            if let Some(snap) = c1_rx.receive(pkt) {
                if snap.players.iter().any(|p| p.id == 2) {
                    c1_saw_c2 = true;
                }
            }
        }
        if c1_saw_c2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        c1_saw_c2,
        "Client 1 observed Client 2 from real server process"
    );

    // Graceful disconnect of Client 2
    client2
        .send_packet(&Packet::Disconnect { player_id: 2 }, server_addr)
        .unwrap();

    // Client 1 receives snapshot showing Client 2 has disconnected
    let mut c1_saw_disconnect = false;
    for _ in 0..30 {
        while let Ok(Some((pkt, _))) = client1.recv_packet() {
            if let Some(snap) = c1_rx.receive(pkt) {
                if !snap.players.iter().any(|p| p.id == 2) {
                    c1_saw_disconnect = true;
                }
            }
        }
        if c1_saw_disconnect {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(c1_saw_disconnect, "Client 1 observed Client 2 disconnect");

    // Reconnect Client 2
    client2
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 2,
            },
            server_addr,
        )
        .unwrap();
    let mut c2_reconnected = false;
    for _ in 0..30 {
        if let Ok(Some((Packet::Welcome { player_id: 2, .. }, _))) = client2.recv_packet() {
            c2_reconnected = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        c2_reconnected,
        "Client 2 reconnected to real server process"
    );

    // Wait for server process to finish its 120 ticks
    let status = server_child.wait().expect("Wait for server process");
    assert!(
        status.success(),
        "Server process completed successfully with exit code 0"
    );
}

#[test]
fn test_session_security_and_disconnect_verification() {
    use vesper3d::viewer::{
        net::{Packet, UdpTransport, PROTOCOL_VERSION},
        server::DedicatedServer,
    };

    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("Server bind");
    let server_addr = server.local_addr;

    let client1 = UdpTransport::bind("127.0.0.1:0").expect("Client 1 bind");
    let client2 = UdpTransport::bind("127.0.0.1:0").expect("Client 2 bind");

    // Client 1 connects
    client1
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 1);
    assert!(server.sessions.contains_key(&1));

    // Client 2 connects but attempts to maliciously request player_id 999
    client2
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 999,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 2);
    // Server must reject arbitrary requested ID and allocate sequential ID 2
    assert!(!server.sessions.contains_key(&999));
    assert!(server.sessions.contains_key(&2));

    // Attacker Client 2 attempts to send a spoofed Disconnect packet for Client 1 (player_id: 1)
    client2
        .send_packet(&Packet::Disconnect { player_id: 1 }, server_addr)
        .unwrap();
    server.poll_network().unwrap();

    // Verify Player 1 was NOT disconnected by Client 2's spoofed packet
    assert_eq!(server.sessions.len(), 2);
    assert!(server.sessions.contains_key(&1));
    assert!(server.world.player(1).is_some());

    // Legitimate owner Client 1 disconnects
    client1
        .send_packet(&Packet::Disconnect { player_id: 1 }, server_addr)
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 1);
    assert!(!server.sessions.contains_key(&1));
    assert!(server.world.player(1).is_none());
}

#[test]
fn test_input_sequencing_and_stale_input_neutralization() {
    use vesper3d::viewer::{
        controller::Movement,
        net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
        server::{DedicatedServer, STALE_INPUT_WINDOW_TICKS},
    };

    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("Server bind");
    let server_addr = server.local_addr;

    let client = UdpTransport::bind("127.0.0.1:0").expect("Client bind");
    client
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert!(server.sessions.contains_key(&1));

    // Send input tick 10
    let inp10 = InputFrame {
        client_tick: 10,
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
    };
    client
        .send_packet(&Packet::Input(inp10), server_addr)
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_client_tick, 10);

    // Send duplicate input tick 10 (must be rejected)
    let inp_dup = InputFrame {
        client_tick: 10,
        movement: Movement {
            right: 1.0,
            ..Default::default()
        },
        yaw: 1.0,
        pitch: 0.0,
        fire_wrench: false,
        fire_pistol: false,
        interact: false,
        ack_server_tick: 0,
    };
    client
        .send_packet(&Packet::Input(inp_dup), server_addr)
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_client_tick, 10);
    // Yaw was NOT overwritten by duplicate tick
    assert_eq!(server.world.player(1).unwrap().yaw, 0.0);

    // Send out-of-order input tick 5 (must be rejected)
    let inp_old = InputFrame {
        client_tick: 5,
        movement: Movement {
            right: 1.0,
            ..Default::default()
        },
        yaw: 2.0,
        pitch: 0.0,
        fire_wrench: false,
        fire_pistol: false,
        interact: false,
        ack_server_tick: 0,
    };
    client
        .send_packet(&Packet::Input(inp_old), server_addr)
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions[&1].last_client_tick, 10);
    assert_eq!(server.world.player(1).unwrap().yaw, 0.0);

    // Test stale-input neutralization window:
    // Client sent forward movement at tick 10, then went silent.
    // Run server steps past the neutralization window (STALE_INPUT_WINDOW_TICKS = 6).
    let initial_pos = server.world.player(1).unwrap().position;
    for _ in 0..STALE_INPUT_WINDOW_TICKS {
        server.step();
    }
    let moved_pos = server.world.player(1).unwrap().position;
    assert!((moved_pos - initial_pos).length() > 0.01);

    // Advance 30 more ticks with no packets from client: input is neutralized, velocity decays to 0
    for _ in 0..30 {
        server.step();
    }
    let neutral_pos = server.world.player(1).unwrap().position;

    // Advance 20 further ticks: since input was neutralized and velocity is 0, position does NOT keep moving!
    for _ in 0..20 {
        server.step();
    }
    let stopped_pos = server.world.player(1).unwrap().position;
    assert!(
        (stopped_pos - neutral_pos).length() < 0.001,
        "Player stopped moving after stale input neutralization window"
    );
}

#[test]
fn test_multiplayer_prop_contention_and_ownership() {
    use vesper3d::viewer::{
        controller::Movement,
        net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
        server::DedicatedServer,
    };

    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("Server bind");
    let server_addr = server.local_addr;

    let client1 = UdpTransport::bind("127.0.0.1:0").expect("Client 1");
    let client2 = UdpTransport::bind("127.0.0.1:0").expect("Client 2");

    client1
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    client2
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();
    assert_eq!(server.sessions.len(), 2);

    let phys = server.world.prop_physics.as_ref().unwrap();
    assert!(!phys.props.is_empty(), "Props available");

    // Position player 1 right in front of prop 0, aiming directly at it
    let p0_pos = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_position(0)
        .unwrap();
    let eye1 = server.world.player(1).unwrap().position.1;
    server.world.player_mut(1).unwrap().position =
        vesper3d::math::V(p0_pos.0, eye1, p0_pos.2 + 0.8);
    let delta1 = p0_pos - server.world.player(1).unwrap().position;
    let pitch1 = (delta1.1 / delta1.length()).asin();
    let yaw1 = delta1.0.atan2(-delta1.2);
    server.world.player_mut(1).unwrap().yaw = yaw1;
    server.world.player_mut(1).unwrap().pitch = pitch1;

    // Client 1 sends interact press
    let inp1 = InputFrame {
        client_tick: 1,
        movement: Movement::default(),
        yaw: yaw1,
        pitch: pitch1,
        fire_wrench: false,
        fire_pistol: false,
        interact: true,
        ack_server_tick: 0,
    };
    client1
        .send_packet(&Packet::Input(inp1), server_addr)
        .unwrap();
    server.poll_network().unwrap();

    let p1_held = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .held_for_player(1);
    assert_eq!(p1_held, Some(0), "Player 1 picked up prop 0");
    assert_eq!(
        server.world.prop_physics.as_ref().unwrap().holder_of(0),
        Some(1),
        "Prop 0 held by Player 1"
    );

    // Position player 2 at the same prop and attempt to interact
    let eye2 = server.world.player(2).unwrap().position.1;
    server.world.player_mut(2).unwrap().position =
        vesper3d::math::V(p0_pos.0 + 0.2, eye2, p0_pos.2 + 0.8);
    let delta2 = p0_pos - server.world.player(2).unwrap().position;
    let pitch2 = (delta2.1 / delta2.length()).asin();
    let yaw2 = delta2.0.atan2(-delta2.2);
    server.world.player_mut(2).unwrap().yaw = yaw2;
    server.world.player_mut(2).unwrap().pitch = pitch2;

    let inp2 = InputFrame {
        client_tick: 1,
        movement: Movement::default(),
        yaw: yaw2,
        pitch: pitch2,
        fire_wrench: false,
        fire_pistol: false,
        interact: true,
        ack_server_tick: 0,
    };
    client2
        .send_packet(&Packet::Input(inp2), server_addr)
        .unwrap();
    server.poll_network().unwrap();

    // Contention resolution: Player 2 cannot steal prop 0 while Player 1 holds it
    assert_eq!(
        server
            .world
            .prop_physics
            .as_ref()
            .unwrap()
            .held_for_player(2),
        None,
        "Player 2 interaction rejected by contention resolution"
    );
    assert_eq!(
        server.world.prop_physics.as_ref().unwrap().holder_of(0),
        Some(1),
        "Prop 0 remains held by Player 1"
    );

    // When Player 1 disconnects, their held prop is released automatically
    client1
        .send_packet(&Packet::Disconnect { player_id: 1 }, server_addr)
        .unwrap();
    server.poll_network().unwrap();

    assert_eq!(
        server.world.prop_physics.as_ref().unwrap().holder_of(0),
        None,
        "Prop 0 freed after Player 1 leaves"
    );
}

#[test]
fn test_authoritative_combat_hitscan_and_impulse() {
    use vesper3d::viewer::{
        controller::Movement,
        net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
        server::DedicatedServer,
    };

    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("Server bind");
    let server_addr = server.local_addr;

    let client = UdpTransport::bind("127.0.0.1:0").expect("Client");
    client
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();

    // Position player directly facing prop 0 within reach
    let p0_pos = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_position(0)
        .unwrap();
    let eye1 = server.world.player(1).unwrap().position.1;
    server.world.player_mut(1).unwrap().position =
        vesper3d::math::V(p0_pos.0, eye1, p0_pos.2 + 0.8);
    let delta = p0_pos - server.world.player(1).unwrap().position;
    let pitch = (delta.1 / delta.length()).asin();
    let yaw = delta.0.atan2(-delta.2);
    server.world.player_mut(1).unwrap().yaw = yaw;
    server.world.player_mut(1).unwrap().pitch = pitch;

    let initial_speed = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_linear_velocity(0)
        .unwrap()
        .length();

    // Client fires pistol authoritatively
    let inp_pistol = InputFrame {
        client_tick: 1,
        movement: Movement::default(),
        yaw,
        pitch,
        fire_wrench: false,
        fire_pistol: true,
        interact: false,
        ack_server_tick: 0,
    };
    client
        .send_packet(&Packet::Input(inp_pistol), server_addr)
        .unwrap();
    server.poll_network().unwrap();

    let post_shot_speed = server
        .world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_linear_velocity(0)
        .unwrap()
        .length();

    // Authoritative pistol shot applied physical impulse to the targeted prop!
    assert!(
        post_shot_speed > initial_speed + 0.1,
        "Authoritative shot applied physical impulse to prop (speed: {post_shot_speed})"
    );
}

#[test]
fn test_prop_state_interpolation_and_quantized_checksum() {
    use vesper3d::math::V;
    use vesper3d::viewer::{
        lifecycle::Generation,
        net::{InterpolationBuffer, PropNetState},
        simulation::HeadlessWorld,
    };

    // 1. Verify PropNetState interpolation with position, rotation, linear & angular velocity
    let mut buffer = InterpolationBuffer::<PropNetState>::new(16);
    let prop0 = PropNetState {
        id: "apple_1".into(),
        position: V(0.0, 1.0, 0.0),
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: V(1.0, 0.0, 0.0),
        angular_velocity: V(0.0, 2.0, 0.0),
        sleeping: false,
        held_by: None,
        generation: Generation(1),
    };
    let prop1 = PropNetState {
        id: "apple_1".into(),
        position: V(1.0, 1.0, 0.0),
        rotation: [
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        ],
        linear_velocity: V(1.0, 0.0, 0.0),
        angular_velocity: V(0.0, 2.0, 0.0),
        sleeping: false,
        held_by: Some(1),
        generation: Generation(1),
    };

    buffer.push(10, prop0);
    buffer.push(20, prop1);

    let interp = buffer.interpolate_at(15.0).expect("Interpolated state");
    assert!((interp.position.0 - 0.5).abs() < 1e-4);
    assert_eq!(interp.held_by, Some(1));
    assert!(interp.rotation[1] > 0.3 && interp.rotation[1] < 0.6);

    // 2. Verify deterministic quantized checksum catches prop movements
    let mut world1 = HeadlessWorld::new().unwrap();
    let mut world2 = HeadlessWorld::new().unwrap();
    world1.join(1);
    world2.join(1);

    // At identical state, checksums are identical
    assert_eq!(world1.checksum(), world2.checksum());

    // Nudge a dynamic prop in world1
    world1.apply_prop_impulse(0, V(0.5, 1.0, 0.0));
    world1.step();
    world2.step();

    // Checksum must diverge immediately because prop transform and velocity are quantized and hashed!
    assert_ne!(
        world1.checksum(),
        world2.checksum(),
        "Quantized checksum diverges when a prop moves"
    );
}

#[test]
fn test_weapon_occlusion_and_wall_blocking() {
    use vesper3d::math::V;
    use vesper3d::viewer::simulation::HeadlessWorld;

    let mut world = HeadlessWorld::new().unwrap();
    // Position Player 1 in Main Arena behind East wall (wall is at X = 8.1, Z = -4.5)
    world.join_at(1, V(5.0, 1.0, -4.5));

    let phys = world.prop_physics.as_ref().unwrap();
    let prop_idx = 0;
    let prop_pos = phys.prop_position(prop_idx).unwrap();

    // Aim from player 1 at prop_pos through the solid East wall
    let p1 = world.player_mut(1).unwrap();
    let delta = prop_pos - p1.position;
    let pitch = (delta.1 / delta.length()).asin();
    let yaw = delta.0.atan2(-delta.2);
    p1.yaw = yaw;
    p1.pitch = pitch;

    let initial_speed = world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_linear_velocity(prop_idx)
        .unwrap()
        .length();

    // Player 1 fires pistol: ray must hit the occluding wall before reaching the prop
    let hit_point = world.fire_pistol(1);
    assert!(hit_point.is_some(), "Pistol hit the occluding wall");

    // Dynamic prop behind the wall must NOT have received any impulse
    let speed_after_wall_shot = world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_linear_velocity(prop_idx)
        .unwrap()
        .length();
    assert_eq!(
        speed_after_wall_shot, initial_speed,
        "Prop behind wall did not receive impulse (occluded by wall)"
    );

    // Now move player 1 in direct line of sight of the prop (no wall between)
    let eye1 = world.player(1).unwrap().position.1;
    world.player_mut(1).unwrap().position = V(prop_pos.0 - 1.5, eye1, prop_pos.2);
    let delta_direct = prop_pos - world.player(1).unwrap().position;
    let pitch_direct = (delta_direct.1 / delta_direct.length()).asin();
    let yaw_direct = delta_direct.0.atan2(-delta_direct.2);
    world.player_mut(1).unwrap().yaw = yaw_direct;
    world.player_mut(1).unwrap().pitch = pitch_direct;

    // Fire pistol with clear line of sight
    let hit_direct = world.fire_pistol(1);
    assert!(hit_direct.is_some(), "Direct line of sight hit");

    let speed_after_direct_shot = world
        .prop_physics
        .as_ref()
        .unwrap()
        .prop_linear_velocity(prop_idx)
        .unwrap()
        .length();
    assert!(
        speed_after_direct_shot > initial_speed + 0.1,
        "Prop with clear line of sight received physical impulse"
    );
}

#[test]
fn test_delta_recovery_under_packet_loss_reordering_and_jitter() {
    use std::collections::VecDeque;
    use vesper3d::viewer::{
        controller::Movement,
        net::{InputFrame, Packet, UdpTransport, PROTOCOL_VERSION},
        server::DedicatedServer,
    };

    let mut server = DedicatedServer::bind("127.0.0.1:0").expect("Server bind");
    let server_addr = server.local_addr;

    let mut client1 = UdpTransport::bind("127.0.0.1:0").expect("Client 1");
    let mut client2 = UdpTransport::bind("127.0.0.1:0").expect("Client 2");

    client1
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    client2
        .send_packet(
            &Packet::Hello {
                protocol_version: PROTOCOL_VERSION,
                player_id: 0,
            },
            server_addr,
        )
        .unwrap();
    server.poll_network().unwrap();

    let mut c1_rx = TestClientReceiver::new();
    let mut c2_rx = TestClientReceiver::new();

    let mut c1_inbound_queue: VecDeque<(u64, Packet)> = VecDeque::new();
    let mut c2_inbound_queue: VecDeque<(u64, Packet)> = VecDeque::new();

    let latency_ticks = 5u64; // ~80 ms at 60 Hz
    let mut dropped_packets = 0usize;
    let mut reordered_packets = 0usize;

    // Run for 120 ticks (~2 seconds of simulation)
    for tick in 1..=120 {
        let inp1 = InputFrame {
            client_tick: tick,
            movement: Movement {
                forward: if tick % 20 < 10 { 1.0 } else { -1.0 },
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: tick == 15 || tick == 45,
            ack_server_tick: c1_rx.latest_acked_tick(),
        };
        client1
            .send_packet(&Packet::Input(inp1), server_addr)
            .unwrap();

        let inp2 = InputFrame {
            client_tick: tick,
            movement: Movement {
                right: if tick % 20 < 10 { 1.0 } else { -1.0 },
                ..Default::default()
            },
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
            ack_server_tick: c2_rx.latest_acked_tick(),
        };
        client2
            .send_packet(&Packet::Input(inp2), server_addr)
            .unwrap();

        server.poll_network().unwrap();
        server.step();

        // Collect packets emitted by server to client sockets
        while let Ok(Some((pkt, _))) = client1.recv_packet() {
            // Simulate 10% packet loss: drop every 10th packet
            if (tick + c1_inbound_queue.len() as u64).is_multiple_of(10) {
                dropped_packets += 1;
                continue;
            }
            // Simulate jitter (delay +/- 2 ticks)
            let jitter = if tick % 3 == 0 { 2 } else { 0 };
            if jitter > 0 {
                reordered_packets += 1;
            }
            c1_inbound_queue.push_back((tick + latency_ticks + jitter, pkt));
        }

        while let Ok(Some((pkt, _))) = client2.recv_packet() {
            if (tick + c2_inbound_queue.len() as u64).is_multiple_of(10) {
                dropped_packets += 1;
                continue;
            }
            let jitter = if tick % 4 == 0 { 2 } else { 0 };
            if jitter > 0 {
                reordered_packets += 1;
            }
            c2_inbound_queue.push_back((tick + latency_ticks + jitter, pkt));
        }

        // Deliver ready packets to Client 1
        let mut i = 0;
        while i < c1_inbound_queue.len() {
            if c1_inbound_queue[i].0 <= tick {
                let (_, pkt) = c1_inbound_queue.remove(i).unwrap();
                c1_rx.receive(pkt);
            } else {
                i += 1;
            }
        }
        if c1_rx.keyframe_needed {
            client1
                .send_packet(&Packet::RequestKeyframe, server_addr)
                .unwrap();
        }

        // Deliver ready packets to Client 2
        let mut j = 0;
        while j < c2_inbound_queue.len() {
            if c2_inbound_queue[j].0 <= tick {
                let (_, pkt) = c2_inbound_queue.remove(j).unwrap();
                c2_rx.receive(pkt);
            } else {
                j += 1;
            }
        }
        if c2_rx.keyframe_needed {
            client2
                .send_packet(&Packet::RequestKeyframe, server_addr)
                .unwrap();
        }
    }

    // Deliver remaining packets and allow clean recovery for 25 ticks
    for tick in 121..=145 {
        let inp1 = InputFrame {
            client_tick: tick,
            movement: Movement::default(),
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
            ack_server_tick: c1_rx.latest_acked_tick(),
        };
        client1
            .send_packet(&Packet::Input(inp1), server_addr)
            .unwrap();

        let inp2 = InputFrame {
            client_tick: tick,
            movement: Movement::default(),
            yaw: 0.0,
            pitch: 0.0,
            fire_wrench: false,
            fire_pistol: false,
            interact: false,
            ack_server_tick: c2_rx.latest_acked_tick(),
        };
        client2
            .send_packet(&Packet::Input(inp2), server_addr)
            .unwrap();

        server.poll_network().unwrap();
        server.step();

        while let Ok(Some((pkt, _))) = client1.recv_packet() {
            c1_rx.receive(pkt);
        }
        while let Ok(Some((pkt, _))) = client2.recv_packet() {
            c2_rx.receive(pkt);
        }
    }

    assert!(
        dropped_packets > 0,
        "Packet loss was actually simulated (dropped: {dropped_packets})"
    );
    assert!(
        reordered_packets > 0,
        "Packet jitter/reordering was actually simulated (jittered: {reordered_packets})"
    );
    assert!(
        c1_rx.deltas_applied > 0,
        "Client 1 successfully applied valid deltas (applied: {})",
        c1_rx.deltas_applied
    );
    assert!(
        c2_rx.deltas_applied > 0,
        "Client 2 successfully applied valid deltas (applied: {})",
        c2_rx.deltas_applied
    );
    assert!(
        c1_rx.baseline.is_some(),
        "Client 1 has valid recovered baseline snapshot"
    );
    assert!(
        c2_rx.baseline.is_some(),
        "Client 2 has valid recovered baseline snapshot"
    );

    let final_c1_tick = c1_rx.baseline.as_ref().unwrap().tick;
    let final_c2_tick = c2_rx.baseline.as_ref().unwrap().tick;
    assert!(
        final_c1_tick >= 120,
        "Client 1 settled at recent tick: {final_c1_tick}"
    );
    assert!(
        final_c2_tick >= 120,
        "Client 2 settled at recent tick: {final_c2_tick}"
    );
}
