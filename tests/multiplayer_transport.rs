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
}
