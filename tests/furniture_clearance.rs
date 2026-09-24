use std::collections::BTreeMap;
use vesper3d::{
    math::V,
    scene::{Scene, Track},
    viewer::{
        authoring::MapDocument,
        camera::{CameraRig, Perspective},
        controller::{CharacterKind, Collider, Controller, Movement},
        interaction::Action,
        prop_physics::PropPhysics,
        props::{self, PropKind},
        room::{Entity, Room},
    },
};

fn fixture(kind: &str, rotated: bool) -> Room {
    let (mut scene, mut half): (Scene, V) = match kind {
        "Chair" => (props::scene(PropKind::Chair), V(0.26, 0.47, 0.26)),
        "Table" => (props::scene(PropKind::Table), V(0.8, 0.4, 0.5)),
        _ => (
            serde_json::from_str(include_str!("../assets/props/interiors/student-desk.json"))
                .unwrap(),
            V(0.58, 0.39, 0.37),
        ),
    };
    if rotated {
        for node in &mut scene.nodes {
            if let (Track::Fixed(p), Track::Fixed(r)) = (&mut node.pos, &mut node.rot) {
                *p = V(p.2, p.1, -p.0);
                r.1 += 90.;
            }
        }
        half = V(half.2, half.1, half.0);
    }
    let bounds = Collider {
        min: V(-half.0, 0., -half.2),
        max: V(half.0, half.1 * 2., half.2),
    };
    MapDocument {
        schema_version: 1,
        name: "Furniture clearance".into(),
        scene,
        colliders: BTreeMap::from([("furniture".into(), bounds.clone())]),
        entities: vec![Entity {
            id: "furniture".into(),
            label: kind.into(),
            bounds,
            action: Action::Inspect,
        }],
        spatial: None,
    }
    .build()
    .unwrap()
}

fn walk(room: &Room, kind: CharacterKind, x: f32) -> Controller {
    let mut p = Controller::for_character(kind);
    p.position.0 = x;
    p.position.2 = 1.3;
    p.yaw = 0.;
    for _ in 0..80 {
        p.update(
            Movement {
                forward: 1.,
                ..Default::default()
            },
            1. / 60.,
            &room.colliders,
        );
    }
    p
}

#[test]
fn feta_passes_beneath_fixed_and_physical_furniture_but_scientist_cannot() {
    for kind in ["Chair", "Table", "Student desk"] {
        for rotated in [false, true] {
            for physical in [false, true] {
                let mut room = fixture(kind, rotated);
                let _physics = physical.then(|| PropPhysics::new(&mut room).unwrap());
                let rat = walk(&room, CharacterKind::Feta, 0.);
                assert!(
                    rat.position.2 < -1.,
                    "{kind} rotated={rotated} physical={physical}: {:?}",
                    rat.position
                );
                assert!(
                    rat.feet_height().abs() < 0.01,
                    "Must use the gap, not climb furniture"
                );
                assert!(walk(&room, CharacterKind::Scientist, 0.).position.2 > 0.2);
            }
        }
    }
}

#[test]
fn furniture_legs_and_undersides_stay_solid() {
    for (kind, leg_x) in [("Chair", 0.21), ("Table", 0.68), ("Student desk", 0.49)] {
        let room = fixture(kind, false);
        assert!(
            walk(&room, CharacterKind::Feta, leg_x).position.2 > 0.,
            "{kind} leg should block"
        );
        let mut rat = Controller::for_character(CharacterKind::Feta);
        rat.position.2 = 0.;
        let ceiling = if kind == "Chair" {
            0.40
        } else if kind == "Table" {
            0.70
        } else {
            0.585
        };
        for tick in 0..60 {
            rat.update(
                Movement {
                    jump: tick == 0,
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
            assert!(
                rat.feet_height() + rat.body_height() <= ceiling + 0.001,
                "{kind} underside should block jumping"
            );
        }
    }
}

#[test]
fn feta_uses_desk_passages_in_every_shipped_map() {
    for (map, id) in [
        ("house", "expansion/workshop/table-0/desk"),
        ("office", "expansion/training/desk-0-0/desk"),
        ("school-wing", "expansion/art/desk-0/desk"),
        ("convenience-store", "expansion/picnic/table-0-0/desk"),
    ] {
        let doc = MapDocument::load(std::path::Path::new(&format!(
            "assets/maps/starters/{map}.json"
        )))
        .unwrap();
        let mut room = doc.build().unwrap();
        let _physics = PropPhysics::new(&mut room).unwrap();
        let bounds = &room.entities.iter().find(|e| e.id == id).unwrap().bounds;
        let mut rat = Controller::for_character(CharacterKind::Feta);
        rat.position.0 = (bounds.min.0 + bounds.max.0) * 0.5;
        rat.position.2 = bounds.min.2 - 0.4;
        rat.yaw = std::f32::consts::PI;
        for _ in 0..60 {
            if rat.position.2 > bounds.max.2 + 0.3 {
                break;
            }
            rat.step(1., 0., false, 1. / 60., &room.colliders);
        }
        assert!(
            rat.position.2 > bounds.max.2 + 0.3,
            "{map}: {:?}",
            rat.position
        );
        assert!(rat.feet_height() < 0.04, "{map}: must pass underneath");
    }
}

#[test]
fn feta_camera_stays_low_and_stable_through_furniture_at_any_look_pitch() {
    for kind in ["Chair", "Table", "Student desk"] {
        for physical in [false, true] {
            for pitch in [-1.2, 0., 1.2] {
                let mut room = fixture(kind, false);
                let _physics = physical.then(|| PropPhysics::new(&mut room).unwrap());
                let mut player = Controller::for_character(CharacterKind::Feta);
                player.position.0 = 0.;
                player.position.2 = 2.;
                player.yaw = 0.;
                player.pitch = pitch;
                let mut rig = CameraRig::default();
                for _ in 0..80 {
                    player.update(
                        Movement {
                            forward: 1.,
                            ..Default::default()
                        },
                        1. / 60.,
                        &room.colliders,
                    );
                    rig.advance(Perspective::Third, &player, &room, 1. / 60.);
                    let view = rig.view(Perspective::Third, &player, &room);
                    assert!(
                        (view.eye - player.position - V(0., 0.06, 1.05)).length() < 0.001,
                        "{kind} physical={physical} pitch={pitch}: boom changed at {:?}",
                        player.position
                    );
                    assert!(view.show_body, "Feta disappeared under furniture");
                    assert!(
                        ((view.target - view.eye).norm() - player.direction()).length() < 0.001
                    );
                }
                assert!(player.position.2 < -2.);
            }
        }
    }
}
#[test]
fn camera_retracts_for_walls_and_releases_smoothly_independent_of_frame_rate() {
    let mut room = fixture("Table", false);
    let mut player = Controller::for_character(CharacterKind::Feta);
    player.position = V(0., 0.22, 3.);
    let wall = Collider {
        min: V(-2., 0., 3.4),
        max: V(2., 2., 3.5),
    };
    let mut results = vec![];
    for rate in [30, 60, 144] {
        let mut rig = CameraRig::default();
        rig.advance(Perspective::Third, &player, &room, 1. / rate as f32);
        room.colliders.push(wall.clone());
        rig.advance(Perspective::Third, &player, &room, 1. / rate as f32);
        let blocked = rig.view(Perspective::Third, &player, &room);
        assert!(blocked.eye.2 < 3.4 - 0.07);
        room.colliders.pop();
        let before = (blocked.eye - player.position).length();
        rig.advance(Perspective::Third, &player, &room, 1. / rate as f32);
        let first = (rig.view(Perspective::Third, &player, &room).eye - player.position).length();
        assert!(first > before && first < 0.6, "camera snapped outward");
        for _ in 1..rate {
            rig.advance(Perspective::Third, &player, &room, 1. / rate as f32);
        }
        results.push((rig.view(Perspective::Third, &player, &room).eye - player.position).length());
        room.colliders.push(wall.clone());
        assert!(rig.view(Perspective::Third, &player, &room).eye.2 < 3.4 - 0.07);
        room.colliders.pop();
        rig.advance(Perspective::First, &player, &room, 0.);
        assert_eq!(
            rig.view(Perspective::First, &player, &room).eye,
            player.position
        );
    }
    assert!(results
        .iter()
        .all(|value| (value - results[0]).abs() < 0.0001));
}
