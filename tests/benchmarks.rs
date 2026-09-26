use std::time::Instant;
use vesper3d::{
    math::V,
    viewer::{
        controller::{Controller, Movement},
        lifecycle::{LifecycleRegistry, LifecycleState},
        metrics::RegressionBudget,
        net::{InputFrame, PlayerNetState, PredictionBuffer},
        simulation::{HeadlessWorld, TICK_SECONDS},
        spatial::RoomGraph,
    },
};

#[test]
fn benchmark_simulation_step_throughput() {
    let mut world = HeadlessWorld::new().expect("Build House");
    world.join(1);
    world.join(2);

    let start = Instant::now();
    let ticks = 500;
    for _ in 0..ticks {
        world.step();
    }
    let elapsed = start.elapsed();
    let mean_us = elapsed.as_secs_f64() * 1_000_000. / ticks as f64;
    println!("Throughput: {ticks} ticks in {elapsed:?} (mean {mean_us:.2} µs/tick)");
    assert_eq!(world.tick, 500);
    assert!(
        mean_us <= RegressionBudget::default().max_sim_step_mean_us,
        "simulation step regression: {mean_us:.3} us exceeds budget"
    );
}

#[test]
fn benchmark_snapshot_and_delta_throughput() {
    let mut world = HeadlessWorld::new().expect("Build House");
    world.join(1);
    world.join(2);
    world.step();

    let snap1 = world.snapshot(world.tick);
    let mut snap2 = snap1.clone();
    snap2.tick += 1;
    if let Some(p) = snap2.players.get_mut(0) {
        p.position.0 += 0.05;
    }

    let iters = 2000;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(world.snapshot(world.tick));
    }
    let snapshot_mean_us = start.elapsed().as_secs_f64() * 1_000_000. / iters as f64;
    println!("Snapshot creation: mean {snapshot_mean_us:.3} µs/op");
    assert!(
        snapshot_mean_us <= RegressionBudget::default().max_snapshot_creation_mean_us,
        "snapshot creation regression: {snapshot_mean_us:.3} us exceeds budget"
    );

    let start = Instant::now();
    for _ in 0..iters {
        let delta = snap2.compute_delta(&snap1);
        let reconstructed = delta.apply_to(&snap1);
        assert_eq!(reconstructed.tick, snap2.tick);
    }
    let mean_us = start.elapsed().as_secs_f64() * 1_000_000. / iters as f64;
    println!("Delta compression roundtrip: mean {mean_us:.3} µs/op");
    assert!(
        mean_us <= RegressionBudget::default().max_delta_compression_mean_us,
        "delta compression regression: {mean_us:.3} us exceeds budget"
    );
}

#[test]
fn benchmark_prediction_and_reconciliation_throughput() {
    let mut pred = PredictionBuffer::new(64);
    let mut controller = Controller::default();
    let colliders = vec![];

    let iters = 1000;
    let start = Instant::now();
    for i in 1..=iters {
        let input = InputFrame {
            client_tick: i as u64,
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
            session_token: None,
        };
        controller.update(input.movement, TICK_SECONDS, &colliders);
        pred.push(input, controller.clone());

        if i % 3 == 0 {
            let mut server_state = PlayerNetState::from_controller(1, i as u64, &controller, None);
            server_state.position.0 += 0.01;
            let _ = pred.reconcile(i as u64, &server_state, &mut controller, &colliders, 0.005);
        }
    }
    let mean_us = start.elapsed().as_secs_f64() * 1_000_000. / iters as f64;
    println!("Prediction + Reconciliation: mean {mean_us:.3} µs/step");
    assert!(controller.position.0.is_finite());
}

#[test]
fn benchmark_room_graph_spatial_queries() {
    let graph = RoomGraph::house();
    let iters = 10000;
    let start = Instant::now();
    for i in 0..iters {
        let x = (i as f32 % 20.0) - 10.0;
        let z = (i as f32 % 20.0) - 10.0;
        let _ = graph.find_room_at(V(x, 1.0, z));
    }
    let mean_ns = start.elapsed().as_secs_f64() * 1_000_000_000. / iters as f64;
    println!("Room graph spatial lookup: mean {mean_ns:.1} ns/query");
    assert!(
        mean_ns <= RegressionBudget::default().max_room_graph_lookup_mean_ns,
        "room graph lookup regression: {mean_ns:.3} ns exceeds budget"
    );
}

#[test]
fn benchmark_lifecycle_promotion_and_counts() {
    let mut reg = LifecycleRegistry::new();
    for i in 0..500 {
        reg.register(format!("prop-{i}"), "Prop".into(), V(i as f32, 0.0, 0.0));
    }
    assert_eq!(reg.counts().0, 500);

    let start = Instant::now();
    for i in 0..500 {
        reg.promote(i, LifecycleState::DynamicEntity);
    }
    let elapsed = start.elapsed();
    println!("Promoted 500 objects in {elapsed:?}");
    assert_eq!(reg.counts().2, 500);
}
