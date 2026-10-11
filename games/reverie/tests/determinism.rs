use reverie::{
    collection::{exhibit, room_seed},
    Event, Input, Sim, Target,
};
use vesper3d::{
    math::V,
    viewer::devkit::{assert_deterministic, snapshot, Simulation, Snapshot},
};
fn press(sim: &mut Sim) {
    sim.step(&Input { interact: true, ..Default::default() });
}
fn approach(sim: &mut Sim, target: Target) {
    let (_, p) = sim.targets().into_iter().find(|(t, _)| *t == target).unwrap();
    let offset = if matches!(target, Target::Art(1)) {
        V(2., 0., 0.)
    } else if matches!(target, Target::Art(2)) {
        V(-2., 0., 0.)
    } else if matches!(target, Target::Parent | Target::Return) {
        V(0., 0., -2.)
    } else {
        V(0., 0., 2.)
    };
    sim.player.position.0 = p.0 + offset.0;
    sim.player.position.2 = p.2 + offset.2;
    sim.face(p);
    assert_eq!(sim.target(), Some(target), "target at {p:?}");
}
fn open_painting(sim: &mut Sim, slot: usize) {
    approach(sim, Target::Art(slot));
    press(sim);
}
fn collect(sim: &mut Sim) {
    if sim.key_host() < 3 {
        open_painting(sim, sim.key_host());
        for _ in 1..sim.key_depth() {
            approach(sim, Target::Nested);
            press(sim);
        }
        approach(sim, Target::Key);
        press(sim);
        while sim.depth() > 0 {
            sim.step(&Input { leave: true, ..Default::default() });
        }
    } else {
        let host = sim.key_host();
        approach(sim, Target::Art(host));
        press(sim);
        approach(sim, Target::Key);
        press(sim);
    }
    assert!(sim.has_key());
}
#[test]
fn key_is_two_paintings_deep_on_first_visit() {
    let mut s = Sim::new(3);
    assert_eq!(s.key_host(), 0);
    assert_eq!(s.key_depth(), 2);
    open_painting(&mut s, 0);
    assert!(!s.key_visible());
    approach(&mut s, Target::Nested);
    press(&mut s);
    assert!(s.key_visible());
    approach(&mut s, Target::Key);
    press(&mut s);
    assert_eq!(s.archive.keys.len(), 1);
    press(&mut s);
    assert_eq!(s.archive.keys.len(), 1);
}
#[test]
fn every_gallery_has_one_reachable_key_and_opens_both_branches() {
    for seed in 1..60 {
        let mut s = Sim::new(seed);
        for branch in [0, 1, 0, 1, 1, 0] {
            collect(&mut s);
            let old = s.archive.room.clone();
            let count = s.archive.keys.len();
            approach(&mut s, Target::Door(branch));
            press(&mut s);
            assert_eq!(s.archive.room.len(), old.len() + 1);
            assert!(!s.has_key());
            approach(&mut s, Target::Parent);
            press(&mut s);
            assert_eq!(s.archive.room, old);
            assert!(s.has_key());
            assert_eq!(s.archive.keys.len(), count);
            approach(&mut s, Target::Door(branch));
            press(&mut s);
        }
    }
}
#[test]
fn locked_door_cannot_change_rooms() {
    let mut s = Sim::new(9);
    approach(&mut s, Target::Door(0));
    press(&mut s);
    assert!(s.archive.room.is_empty());
    assert!(s.drain_events().contains(&Event::Locked));
}
#[test]
fn wrong_painting_never_exposes_the_key() {
    let mut s = Sim::new(3);
    open_painting(&mut s, 1);
    for _ in 0..8 {
        assert!(!s.key_visible());
        approach(&mut s, Target::Nested);
        press(&mut s);
    }
    assert_eq!(s.depth(), 9);
}
#[test]
fn recursive_returns_restore_every_pose_in_order() {
    let mut s = Sim::new(3);
    open_painting(&mut s, 0);
    for _ in 0..12 {
        approach(&mut s, Target::Nested);
        press(&mut s);
    }
    while s.depth() > 0 {
        let pose = *s.archive.returns.last().unwrap();
        s.step(&Input { leave: true, ..Default::default() });
        assert_eq!(s.player.network_state(), pose);
    }
    assert_eq!(s.archive.worlds.len(), 13);
    assert!(s.archive.returns.is_empty());
}
#[test]
fn waking_sculptures_reveals_only_the_assigned_key() {
    let mut s = Sim::new(3);
    collect(&mut s);
    loop {
        approach(&mut s, Target::Door(0));
        press(&mut s);
        if s.key_host() >= 3 {
            break;
        }
        collect(&mut s);
    }
    assert!(!s.key_visible());
    let host = s.key_host();
    let other = 3 + (host - 3 + 1) % 3;
    approach(&mut s, Target::Art(other));
    press(&mut s);
    assert!(!s.key_visible());
    approach(&mut s, Target::Art(host));
    press(&mut s);
    assert!(s.key_visible());
}
#[test]
fn walking_and_collision_use_real_player_intentions() {
    let mut s = Sim::new(3);
    for _ in 0..300 {
        s.step(&Input { forward: 1., ..Default::default() });
    }
    assert!(s.player.position.2 < -5.);
    assert_eq!(s.target(), Some(Target::Art(0)));
    press(&mut s);
    assert_eq!(s.depth(), 1);
    for _ in 0..350 {
        s.step(&Input { forward: 1., ..Default::default() });
    }
    assert!(s.player.position.2 < -5.);
    for _ in 0..900 {
        s.step(&Input { right: 1., ..Default::default() });
    }
    assert!(s.player.position.0 < 23.);
}
#[test]
fn interactions_require_proximity_and_aim() {
    let mut s = Sim::new(3);
    press(&mut s);
    assert_eq!(s.depth(), 0);
    s.player.position.2 = -6.;
    s.face(V(0., 2.8, -8.7));
    assert_eq!(s.target(), Some(Target::Art(0)));
    s.player.yaw += std::f32::consts::PI;
    press(&mut s);
    assert_eq!(s.depth(), 0);
}
#[test]
fn stories_artists_frames_and_names_are_present_and_reproducible() {
    for seed in 0..100 {
        for slot in 0..6 {
            let a = exhibit(room_seed(seed, &[0, 1]), slot, 0);
            assert_eq!(a, exhibit(room_seed(seed, &[0, 1]), slot, 0));
            assert!(a.name.len() > 10 && a.artist.len() > 3 && a.story.len() > 100);
            assert!(a.frame < 5);
            assert_ne!(a.seed, exhibit(room_seed(seed, &[1, 0]), slot, 0).seed);
        }
    }
}
#[test]
fn exact_saves_resume_inside_nested_paintings_with_exploration_and_keys() {
    let inputs: Vec<_> = (0..600)
        .map(|t| Input {
            forward: if t < 300 { 1. } else { 0. },
            look: [if t % 60 == 0 { 0.01 } else { 0. }, 0.],
            interact: t == 303,
            ..Default::default()
        })
        .collect();
    assert_deterministic(|| Sim::new(3), &inputs);
    snapshot::assert_resumes_as_promised(|| Sim::new(3), &inputs, 41);
    let mut s = Sim::new(3);
    open_painting(&mut s, 0);
    approach(&mut s, Target::Nested);
    press(&mut s);
    let bytes = snapshot::save(&s, "inside two paintings").unwrap();
    let hash = s.state_hash();
    let mut restored = Sim::new(99);
    snapshot::restore(&mut restored, &bytes).unwrap();
    assert_eq!(hash, restored.state_hash());
    assert_eq!(restored.depth(), 2);
    approach(&mut restored, Target::Key);
    press(&mut restored);
    assert!(restored.has_key());
    let before = restored.state_hash();
    let mut bad = bytes.clone();
    bad[80] ^= 3;
    assert!(snapshot::restore(&mut restored, &bad).is_err());
    assert_eq!(before, restored.state_hash());
}
#[test]
fn invalid_archives_are_refused_without_mutating_the_game() {
    let mut s = Sim::new(3);
    let hash = s.state_hash();
    let mut data = s.capture();
    data.archive.inside.push(0);
    assert!(s.restore(data).is_err());
    assert_eq!(hash, s.state_hash());
}
#[test]
fn branching_paths_do_not_run_out_at_a_machine_integer_boundary() {
    let path = vec![0; 2000];
    assert_ne!(room_seed(3, &path), room_seed(3, &vec![0; 2001]));
}
