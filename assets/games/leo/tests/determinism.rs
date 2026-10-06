use leo::{meadow, Input, Sim, CHUNK_SIZE};
use vesper3d::viewer::devkit::{assert_deterministic, procedural::ChunkId, snapshot, Simulation, Snapshot};
fn inputs() -> Vec<Input> {
    (0..2400)
        .map(|i| Input {
            forward: 1.,
            sprint: true,
            right: if i % 400 < 200 { 0.3 } else { -0.3 },
            jump: i % 200 == 50,
            look: [if i % 400 == 0 { 0.5 } else { 0. }, 0.],
        })
        .collect()
}
#[test]
fn walk_streaming_and_save_resume_are_deterministic() {
    assert_deterministic(|| Sim::new(7), &inputs());
    snapshot::assert_resumes_as_promised(|| Sim::new(7), &inputs(), 1777);
}
#[test]
fn far_world_travel_rebases_without_losing_small_movement() {
    let mut sim = Sim::new(7);
    let mut state = sim.capture();
    state.origin = ChunkId { x: 1_000_000_000_000, z: -1_000_000_000_000 };
    state.player.position.0 = 16.;
    state.player.position.2 = 0.1;
    sim.restore(state).unwrap();
    for _ in 0..6000 {
        sim.step(&Input { forward: 1., sprint: true, ..Default::default() }).unwrap();
    }
    assert!(sim.origin.z < -1_000_000_000_000);
    assert!(sim.player.position.0 >= 0. && sim.player.position.0 < CHUNK_SIZE);
    assert!(sim.player.position.2 >= 0. && sim.player.position.2 < CHUNK_SIZE);
    assert_eq!(sim.chunks.chunks().len(), 49);
    assert!((sim.player.position - sim.previous).length() < 0.2);
}
#[test]
fn world_identity_and_real_plant_variety_survive_revisits() {
    let id = ChunkId { x: -44, z: 99 };
    let a = meadow(7, id).unwrap();
    assert_eq!(a, meadow(7, id).unwrap());
    assert_ne!(a, meadow(8, id).unwrap());
    let kinds: std::collections::BTreeSet<_> =
        (-5..=5).flat_map(|x| meadow(7, ChunkId { x, z: x }).unwrap().plants.into_iter().map(|p| p.kind)).collect();
    assert_eq!(kinds.len(), 9);
}
#[test]
fn repeated_days_and_invalid_loads_keep_authoritative_time() {
    let mut sim = Sim::with_day(4, 240).unwrap();
    for _ in 0..720 {
        sim.step(&Input::default()).unwrap();
    }
    assert_eq!(sim.time().days, 3);
    let before = sim.state_hash();
    let mut invalid = sim.capture();
    invalid.player.position.0 = f32::NAN;
    assert!(sim.restore(invalid).is_err());
    assert_eq!(sim.state_hash(), before);
    let mut invalid = sim.capture();
    invalid.origin.x = i64::MAX;
    assert!(sim.restore(invalid).is_err());
    assert_eq!(sim.state_hash(), before);
    let bytes = snapshot::save(&sim, "third sunrise").unwrap();
    let mut loaded = Sim::new(99);
    snapshot::restore(&mut loaded, &bytes).unwrap();
    assert_eq!(loaded.time(), sim.time());
    assert_eq!(loaded.state_hash(), sim.state_hash());
}
