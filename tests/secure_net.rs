use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use vesper3d::{
    math::V,
    viewer::{
        controller::{CharacterKind, Controller, ControllerState, KinematicState, Movement},
        metrics::{FixedTickRunner, TickMetrics},
        net::{
            action_counters::{ActionCounters, ActionCountersTracker},
            lag_compensation::PoseHistory,
            quic::{Identity, SecureSocket},
            reliable_command::ReliableCommandQueue,
            session::{random_token, HandshakeLimiter, SessionRegistry},
        },
    },
};

#[test]
fn controller_state_restores_complete_kinematic_state() {
    let mut c1 = Controller::for_character(CharacterKind::Scientist);
    c1.position = V(1.2, 3.4, 5.6);
    c1.yaw = 0.78;
    c1.pitch = -0.25;

    // Run a step with movement to populate velocity, feet, etc.
    let colliders = vec![];
    c1.update(
        Movement {
            forward: 1.0,
            jump: true,
            ..Default::default()
        },
        1.0 / 60.0,
        &colliders,
    );

    let state: ControllerState = c1.network_state();
    assert_eq!(state.position, c1.position);
    assert_eq!(state.yaw, c1.yaw);
    assert_eq!(state.pitch, c1.pitch);

    // Verify alias KinematicState is identical
    let _: KinematicState = state;

    // Verify JSON serialization round-trip
    let json = serde_json::to_string(&state).expect("serialize");
    let deserialized: ControllerState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(state, deserialized);

    // Restore into a completely clean controller
    let mut c2 = Controller::for_character(CharacterKind::Scientist);
    c2.restore_network_state(&deserialized);

    assert_eq!(c2.position, c1.position);
    assert_eq!(c2.yaw, c1.yaw);
    assert_eq!(c2.pitch, c1.pitch);
    assert_eq!(c2.is_grounded(), c1.is_grounded());
    assert_eq!(c2.vertical_velocity(), c1.vertical_velocity());
}

#[test]
fn action_counters_prevent_dropped_edge_loss() {
    let mut tracker = ActionCountersTracker::new();
    let mut client_counters = ActionCounters::new();

    // Frame 1: Client jumps
    client_counters.jump += 1;
    let edges = tracker.update(&client_counters);
    assert!(edges.jump);
    assert!(!edges.primary);

    // Frame 2: Packet retransmitted with same counter (e.g. UDP duplicate)
    let edges2 = tracker.update(&client_counters);
    assert!(!edges2.jump, "Duplicate packet must not re-trigger edge");

    // Frame 3: Client fires primary weapon 3 times while 2 intermediate packets were dropped
    client_counters.primary += 3;
    let edges3 = tracker.update(&client_counters);
    assert!(edges3.primary, "New counter must trigger edge");
    assert!(!edges3.jump);

    // Frame 4: Interact incremented
    client_counters.interact += 1;
    let edges4 = tracker.update(&client_counters);
    assert!(edges4.interact);
}

#[test]
fn pose_history_lag_compensation_window_clamping() {
    let mut history = PoseHistory::new(30);

    // Simulate 20 ticks of recorded positions (e.g. entity running along X axis)
    for tick in 1..=20 {
        let pos = V(tick as f32 * 0.5, 1.0, 0.0);
        history.record(tick, pos);
    }

    assert_eq!(history.len(), 20);
    assert_eq!(history.latest_tick(), Some(20));
    assert_eq!(history.oldest_tick(), Some(1));

    let current_server_tick = 20;
    let max_rewind = 8; // Max allowed rewind: tick 12

    // Client viewed target at tick 15 (within allowed window)
    let historical_pose = history.clamped(current_server_tick, 15, max_rewind);
    assert_eq!(historical_pose, Some(&V(7.5, 1.0, 0.0)));

    // Cheater / high-latency client requests tick 5 (older than allowed 8 ticks rewind)
    // Server must clamp to tick 12
    let clamped_pose = history.clamped(current_server_tick, 5, max_rewind);
    assert_eq!(clamped_pose, Some(&V(6.0, 1.0, 0.0))); // tick 12 * 0.5 = 6.0
}

#[test]
fn session_registry_authentication_and_sequencing() {
    let now = Instant::now();
    let mut registry = SessionRegistry::<String>::new(2, Duration::from_secs(5));
    let peer_a: SocketAddr = "127.0.0.1:5001".parse().unwrap();
    let peer_b: SocketAddr = "127.0.0.1:5002".parse().unwrap();
    let peer_c: SocketAddr = "127.0.0.1:5003".parse().unwrap();

    let nonce_a = random_token().unwrap();
    let nonce_b = random_token().unwrap();

    let token_a = registry
        .register(peer_a, nonce_a, now, "PlayerA".into())
        .unwrap();
    let _token_b = registry
        .register(peer_b, nonce_b, now, "PlayerB".into())
        .unwrap();

    assert_eq!(registry.count(), 2);
    assert!(registry.is_full());

    // Capacity limit prevents 3rd player
    assert!(registry
        .register(peer_c, random_token().unwrap(), now, "PlayerC".into())
        .is_err());

    // Sequence validation
    assert!(registry.accept_input_seq(&token_a, 1, now));
    assert!(registry.accept_input_seq(&token_a, 2, now));
    assert!(
        !registry.accept_input_seq(&token_a, 2, now),
        "Duplicate input sequence must be rejected"
    );
    assert!(
        !registry.accept_input_seq(&token_a, 1, now),
        "Stale input sequence must be rejected"
    );

    // Timeout eviction
    let future = now + Duration::from_secs(6);
    let evicted = registry.evict_timeouts(future);
    assert_eq!(evicted.len(), 2);
    assert_eq!(registry.count(), 0);
}

#[test]
fn handshake_rate_limiter_restricts_bursts() {
    let now = Instant::now();
    let mut limiter = HandshakeLimiter::new(3);

    assert!(limiter.allow(now));
    assert!(limiter.allow(now));
    assert!(limiter.allow(now));
    assert!(!limiter.allow(now), "4th handshake in same second must be rejected");

    // 1 second later, window resets
    let next_sec = now + Duration::from_millis(1100);
    assert!(limiter.allow(next_sec));
}

#[test]
fn reliable_command_queue_retention_and_ack() {
    let mut queue = ReliableCommandQueue::<String>::new(4);
    assert_eq!(queue.push("Action1".into()), Some(1));
    assert_eq!(queue.push("Action2".into()), Some(2));
    assert_eq!(queue.push("Action3".into()), Some(3));
    assert_eq!(queue.len(), 3);

    assert_eq!(queue.front().unwrap().command, "Action1");
    assert_eq!(queue.front().unwrap().sequence, 1);

    // Server acks sequence 1
    queue.acknowledge(1);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.front().unwrap().command, "Action2");
    assert_eq!(queue.front().unwrap().sequence, 2);

    // Server acks sequence 3 (cumulative ack)
    queue.acknowledge(3);
    assert_eq!(queue.len(), 0);
    assert!(queue.is_empty());
}

#[test]
fn fixed_tick_runner_and_metrics() {
    let mut metrics = TickMetrics::new();
    metrics.record(1000);
    metrics.record(3000);
    assert_eq!(metrics.count, 2);
    assert_eq!(metrics.mean_us(), 2000);
    assert_eq!(metrics.max_us, 3000);

    let mut runner = FixedTickRunner::new(100);
    let mut step_count = 0;
    for _ in 0..5 {
        runner
            .step(|| {
                step_count += 1;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(step_count, 5);
}

#[test]
fn secure_quic_encrypted_datagram_exchange_and_pinned_cert_enforcement() {
    // Generate authoritative server certificate and key
    let identity = rcgen::generate_simple_self_signed(vec!["feta.local".into()]).unwrap();
    let certificate = identity.cert.der().to_vec();
    let private_key = identity.signing_key.serialize_der();

    let server_identity = Identity::from_der(certificate.clone(), private_key);
    let server_socket =
        SecureSocket::server("127.0.0.1:0".parse().unwrap(), server_identity).unwrap();
    let server_addr = server_socket.local_addr();

    // 1. Untrusted client connecting with impostor certificate must fail closed
    let impostor = rcgen::generate_simple_self_signed(vec!["feta.local".into()]).unwrap();
    let untrusted_client =
        SecureSocket::client(server_addr, impostor.cert.der().to_vec()).unwrap();

    let _ = untrusted_client.send_to(b"untrusted hello", server_addr);
    std::thread::sleep(Duration::from_millis(50));
    let err = untrusted_client.receive();
    assert!(
        err.is_err(),
        "Untrusted client with non-matching certificate must be rejected"
    );

    // 2. Trusted client connecting with pinned server certificate
    let trusted_client = SecureSocket::client(server_addr, certificate.clone()).unwrap();

    // Send payload through DatagramTransport
    let test_payload = b"authenticated blue engine datagram";
    assert_eq!(
        trusted_client.send_to(test_payload, server_addr).unwrap(),
        test_payload.len()
    );

    // Server receives
    let mut received = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        let packets = server_socket.receive().unwrap();
        if !packets.is_empty() {
            received.extend(packets);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(received.len(), 1);
    assert_eq!(received[0].1, test_payload);

    // Server responds back to client
    let client_addr = received[0].0;
    let reply = b"authoritative server snapshot payload";
    server_socket.send_to(reply, client_addr).unwrap();

    let mut client_received = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        let packets = trusted_client.receive().unwrap();
        if !packets.is_empty() {
            client_received.extend(packets);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(client_received.len(), 1);
    assert_eq!(client_received[0].1, reply);
}
