//! Small, low-poly two-story house. All rooms and the garden share one world.
use super::{
    props::{self, PropKind},
    room::{Builder, Room},
};
use crate::{
    geometry::Compiled,
    math::V,
    scene::{Scene, Shape},
};
use std::path::Path;

fn solid(b: &mut Builder, mat: &str, p: V, s: V) {
    b.cube(mat, p, s);
    b.obstacle(p, s);
}
fn furniture(b: &mut Builder, id: &'static str, label: &'static str, p: V, s: V) {
    b.obstacle(p, s);
    b.entity(id, label, p, s);
}
// Wall along X or Z with actual door/window openings, not coplanar decals.
fn facade(b: &mut Builder, along_x: bool, fixed: f32, base: f32, door: bool) {
    let point = |u: f32, y: f32| {
        if along_x {
            V(u, base + y, fixed)
        } else {
            V(fixed, base + y, u)
        }
    };
    let size = |u: f32, y: f32| {
        if along_x {
            V(u, y, 0.10)
        } else {
            V(0.10, y, u)
        }
    };
    let mut holes = vec![(-4.6, -2.4, false), (2.4, 4.6, false)];
    if door {
        holes.insert(1, (-0.85, 0.85, true));
    }
    let mut cursor = -6.;
    for (lo, hi, is_door) in holes {
        solid(
            b,
            "wall",
            point((cursor + lo) * 0.5, 1.6),
            size((lo - cursor) * 0.5, 1.6),
        );
        let lower = if is_door { 0. } else { 1.0 };
        let upper = if is_door { 2.45 } else { 2.35 };
        if lower > 0. {
            solid(
                b,
                "wall",
                point((lo + hi) * 0.5, lower * 0.5),
                size((hi - lo) * 0.5, lower * 0.5),
            );
        }
        solid(
            b,
            "wall",
            point((lo + hi) * 0.5, (upper + 3.2) * 0.5),
            size((hi - lo) * 0.5, (3.2 - upper) * 0.5),
        );
        // The window is open visually, but has an invisible collision barrier.
        if !is_door {
            b.obstacle(
                point((lo + hi) * 0.5, (lower + upper) * 0.5),
                size((hi - lo) * 0.5, (upper - lower) * 0.5),
            );
            for u in [lo + 0.045, hi - 0.045, (lo + hi) * 0.5] {
                b.cube(
                    "white",
                    point(u, (lower + upper) * 0.5),
                    size(0.045, (upper - lower) * 0.5),
                );
            }
            for y in [lower + 0.04, upper - 0.04] {
                b.cube(
                    "white",
                    point((lo + hi) * 0.5, y),
                    size((hi - lo) * 0.5, 0.04),
                );
            }
        }
        cursor = hi;
    }
    solid(
        b,
        "wall",
        point((cursor + 6.) * 0.5, 1.6),
        size((6. - cursor) * 0.5, 1.6),
    );
}
fn bed(b: &mut Builder, x: f32, z: f32, id: &'static str) {
    b.cube("wood", V(x, 3.4, z), V(0.85, 0.20, 1.15));
    b.cube("white", V(x, 3.68, z), V(0.83, 0.08, 1.13));
    b.cube("blue", V(x, 3.79, z + 0.3), V(0.83, 0.03, 0.80));
    b.cube("white", V(x, 3.80, z - 0.8), V(0.60, 0.04, 0.25));
    b.cube("wood", V(x, 3.85, z - 1.21), V(0.88, 0.65, 0.06));
    furniture(b, id, "Bed", V(x, 3.63, z), V(0.88, 0.43, 1.28));
}
pub fn build() -> crate::Result<Room> {
    let mut b = Builder {
        scene: Scene::default(),
        colliders: vec![],
        entities: vec![],
    };
    for (name, c) in [
        ("wall", V(0.82, 0.76, 0.62)),
        ("white", V(0.88, 0.89, 0.83)),
        ("wood", V(0.45, 0.25, 0.12)),
        ("floor", V(0.61, 0.40, 0.22)),
        ("blue", V(0.055, 0.25, 0.48)),
        ("roof", V(0.13, 0.19, 0.24)),
        ("grass", V(0.16, 0.36, 0.10)),
        ("stone", V(0.48, 0.51, 0.49)),
        ("fence", V(0.61, 0.49, 0.31)),
        ("dark", V(0.035, 0.05, 0.055)),
        ("leaf", V(0.12, 0.30, 0.08)),
        ("tile", V(0.53, 0.70, 0.70)),
    ] {
        b.material(name, c, 0., 0.);
    }
    props::palette(&mut b);
    b.cube("grass", V(0., -0.075, -3.), V(9.5, 0.06, 11.5));
    solid(&mut b, "floor", V(0., -0.08, 0.), V(6., 0.08, 6.));
    // Upstairs floor leaves a continuous stairwell on the east side.
    solid(&mut b, "floor", V(-1.35, 3.12, 0.), V(4.65, 0.08, 6.));
    solid(&mut b, "floor", V(4.65, 3.12, -4.3), V(1.35, 0.08, 1.7));
    solid(&mut b, "floor", V(4.65, 3.12, 5.1), V(1.35, 0.08, 0.9));
    solid(&mut b, "white", V(0., 6.48, 0.), V(6., 0.08, 6.));
    for floor in [0., 3.2] {
        for z in [-6., 6.] {
            facade(&mut b, true, z, floor, floor == 0.);
        }
        for x in [-6., 6.] {
            facade(&mut b, false, x, floor, false);
        }
    }
    for (x, angle) in [(-3., 18.), (3., -18.)] {
        b.add(
            Shape::Box,
            "roof",
            V(x, 7.65, 0.),
            V(3.25, 0.12, 6.4),
            V(0., 0., angle),
        );
    }
    // Ground-floor partition: living room and kitchen with a wide passage.
    solid(&mut b, "wall", V(-4.95, 1.55, 0.), V(1.05, 1.55, 0.08));
    solid(&mut b, "wall", V(-1.1, 1.55, 0.), V(0.7, 1.55, 0.08));
    solid(&mut b, "wall", V(-2.85, 2.78, 0.), V(1.05, 0.32, 0.08));
    // Stairway: thirty-two 10 cm risers, ascending toward the backyard.
    for i in 0..32 {
        let top = (i + 1) as f32 * 0.1;
        solid(
            &mut b,
            "wood",
            V(4.5, top * 0.5, 3.7 - i as f32 * 0.2),
            V(0.8, top * 0.5, 0.10),
        );
        if i % 4 == 0 {
            solid(
                &mut b,
                "white",
                V(5.45, top + 0.45, 3.7 - i as f32 * 0.2),
                V(0.035, 0.45, 0.035),
            );
        }
    }
    // Protect upstairs stair opening, leaving the top landing open.
    solid(&mut b, "white", V(3.28, 3.7, 0.8), V(0.045, 0.50, 3.4));
    solid(&mut b, "white", V(4.65, 3.7, 4.18), V(1.35, 0.50, 0.045));
    // Upstairs: front hall, large bedroom west, bedroom and bathroom east.
    solid(&mut b, "wall", V(-0.5, 4.75, -2.), V(0.08, 1.55, 4.));
    for (x, half) in [(-4.15, 1.85), (-0.65, 0.25), (0.75, 1.25)] {
        solid(&mut b, "wall", V(x, 4.75, 2.), V(half, 1.55, 0.08));
    }
    solid(&mut b, "wall", V(-1.45, 6.0, 2.), V(0.55, 0.30, 0.08));
    // East corridor wall with two doorways.
    for (z, half) in [(-5.3, 0.7), (-1.75, 1.65), (1.8, 0.2)] {
        solid(&mut b, "wall", V(2., 4.75, z), V(0.08, 1.55, half));
    }
    for z in [-4., 0.6] {
        solid(&mut b, "wall", V(2., 6.0, z), V(0.08, 0.30, 0.60));
    }
    solid(&mut b, "wall", V(0.75, 4.75, -2.), V(1.25, 1.55, 0.08));
    // Living room furnishings; shapes intentionally simple.
    b.cube("blue", V(-4.7, 0.5, 3.2), V(0.7, 0.25, 1.5));
    b.cube("blue", V(-5.3, 0.90, 3.2), V(0.10, 0.55, 1.5));
    for z in [1.7, 4.7] {
        b.cube("blue", V(-4.7, 0.70, z), V(0.7, 0.45, 0.10));
    }
    furniture(
        &mut b,
        "house-sofa",
        "Sofa",
        V(-4.7, 0.65, 3.2),
        V(0.75, 0.65, 1.6),
    );
    solid(&mut b, "wood", V(-1.45, 0.35, 3.2), V(0.35, 0.35, 1.));
    solid(&mut b, "dark", V(-1.45, 1.2, 3.2), V(0.08, 0.5, 0.8));
    b.entity(
        "house-tv",
        "Television",
        V(-1.45, 1.2, 3.2),
        V(0.08, 0.5, 0.8),
    );
    // Kitchen counters, refrigerator, sink and hob.
    solid(&mut b, "white", V(-5.3, 0.46, -3.9), V(0.55, 0.46, 1.65));
    b.cube("stone", V(-5.3, 0.96, -3.9), V(0.57, 0.04, 1.67));
    b.cube("dark", V(-5.3, 1.008, -3.1), V(0.4, 0.008, 0.4));
    solid(&mut b, "white", V(-3.9, 1., -5.35), V(0.5, 1., 0.5));
    b.cube("dark", V(-3.38, 1.15, -5.1), V(0.02, 0.20, 0.025));
    furniture(
        &mut b,
        "house-fridge",
        "Refrigerator",
        V(-3.9, 1., -5.35),
        V(0.5, 1., 0.5),
    );
    props::place(
        &mut b,
        PropKind::CerealBox,
        "house-cereal",
        "Cereal box",
        V(-2.55, 0.8, -3.4),
    );
    props::place(
        &mut b,
        PropKind::Apple,
        "house-apple",
        "Apple",
        V(-1.8, 0.8, -3.4),
    );
    props::place(
        &mut b,
        PropKind::Table,
        "house-dining-table",
        "Dining table",
        V(-2.2, 0., -3.4),
    );
    for (id, x, z) in [
        ("house-chair-1", -3.35, -3.4),
        ("house-chair-2", -1.05, -3.4),
    ] {
        props::place(&mut b, PropKind::Chair, id, "Dining chair", V(x, 0., z));
    }
    bed(&mut b, -3.8, -3.3, "house-bed-1");
    bed(&mut b, 0.7, -0.65, "house-bed-2");
    solid(&mut b, "wood", V(-5.35, 4.1, 0.5), V(0.45, 0.9, 0.8));
    // Bathroom fittings sit on the upstairs floor.
    solid(&mut b, "tile", V(0.7, 3.215, -4.0), V(1.1, 0.015, 1.9));
    solid(&mut b, "white", V(0.3, 3.5, -5.1), V(0.5, 0.30, 0.65));
    b.cube("tile", V(0.3, 3.81, -5.1), V(0.38, 0.01, 0.53));
    solid(&mut b, "white", V(0.1, 3.58, -2.7), V(0.26, 0.38, 0.38));
    solid(&mut b, "white", V(1.3, 3.63, -5.3), V(0.36, 0.43, 0.40));
    b.entity(
        "house-bath",
        "Bathroom",
        V(0.3, 3.5, -5.1),
        V(0.5, 0.3, 0.65),
    );
    // Backyard patio and reusable picnic table.
    b.cube("stone", V(0., -0.015, -7.6), V(2.4, 0.015, 1.5));
    props::place(
        &mut b,
        PropKind::Table,
        "yard-table",
        "Patio table",
        V(-3.4, 0., -10.),
    );
    props::place(
        &mut b,
        PropKind::Chair,
        "yard-chair",
        "Patio chair",
        V(-4.6, 0., -10.),
    );
    for (x, z) in [(6.8, -11.5), (-7., -12.)] {
        solid(&mut b, "wood", V(x, 1.1, z), V(0.20, 1.1, 0.20));
        b.add(
            Shape::Sphere,
            "leaf",
            V(x, 2.9, z),
            V(1.3, 1.25, 1.3),
            V::ZERO,
        );
    }
    // Picket fence with solid collision boundaries; no escape through visual gaps.
    for x in [-9.5, 9.5] {
        b.obstacle(V(x, 0.95, -3.), V(0.08, 0.95, 11.5));
        for i in 0..58 {
            b.cube(
                "fence",
                V(x, 0.95, -14.5 + i as f32 * 0.4),
                V(0.05, 0.95, 0.16),
            );
        }
        for y in [0.4, 1.3] {
            b.cube("wood", V(x, y, -3.), V(0.08, 0.055, 11.5));
        }
    }
    for z in [-14.5, 8.5] {
        b.obstacle(V(0., 0.95, z), V(9.5, 0.95, 0.08));
        for i in 0..48 {
            b.cube(
                "fence",
                V(-9.4 + i as f32 * 0.4, 0.95, z),
                V(0.16, 0.95, 0.05),
            );
        }
        for y in [0.4, 1.3] {
            b.cube("wood", V(0., y, z), V(9.5, 0.055, 0.08));
        }
    }
    b.cube("stone", V(0., -0.015, 7.2), V(1.0, 0.015, 1.1));
    let compiled = Compiled::new(b.scene, Path::new("."))?;
    let world = compiled.at(0.);
    Ok(Room {
        name: "Suburban House",
        simple_geometry: true,
        compiled,
        world,
        colliders: b.colliders,
        entities: b.entities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::controller::{Controller, Movement};
    fn walk_to(p: &mut Controller, room: &Room, x: f32, z: f32) {
        for _ in 0..480 {
            let d = V(x - p.position.0, 0., z - p.position.2);
            if d.length() < 0.08 {
                p.stop();
                return;
            }
            p.yaw = d.0.atan2(-d.2);
            p.update(
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        }
        panic!("Blocked reaching ({x},{z}) at {:?}", p.position);
    }
    #[test]
    fn every_upstairs_room_has_a_walkable_route() {
        let room = build().unwrap();
        let mut p = Controller::default();
        for (x, z) in [
            (4.5, 4.1),
            (4.5, -3.1),
            (2.8, -3.1),
            (2.8, -4.),
            (1.25, -4.),
            (2.8, -4.),
            (2.8, 1.05),
            (1.0, 1.05),
            (2.8, 1.05),
            (2.8, 3.),
            (-1.6, 3.),
            (-1.6, 1.),
            (-2.7, 0.),
        ] {
            walk_to(&mut p, &room, x, z);
        }
        assert!((p.feet_height() - 3.2).abs() < 0.01);
    }
    #[test]
    fn spawn_entities_and_mesh_budget_are_valid() {
        let room = build().unwrap();
        assert!(!room
            .colliders
            .iter()
            .any(|c| c.blocks(Controller::default().position)));
        let ids: std::collections::HashSet<_> = room.entities.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), room.entities.len());
        assert!(ids.contains("house-apple") && ids.contains("yard-table"));
        assert!(room.world.instances.len() < 500);
    }
    #[test]
    fn stairs_reach_second_floor_and_return_without_jumping() {
        let room = build().unwrap();
        let mut p = Controller::default();
        p.position.0 = 4.5;
        p.position.2 = 4.1;
        p.yaw = 0.;
        for _ in 0..180 {
            p.update(
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        }
        assert!(
            p.feet_height() > 3.19,
            "feet {} pos {:?}",
            p.feet_height(),
            p.position
        );
        assert!(p.position.2 < -2.6);
        p.yaw = std::f32::consts::PI;
        for _ in 0..190 {
            p.update(
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        }
        assert!(p.feet_height() < 0.11, "feet {}", p.feet_height());
    }
    #[test]
    fn back_door_opens_to_yard_and_fence_stops_player() {
        let room = build().unwrap();
        let mut p = Controller::default();
        p.yaw = 0.;
        for _ in 0..600 {
            p.update(
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                1. / 60.,
                &room.colliders,
            );
        }
        assert!(
            p.position.2 < -12. && p.position.2 > -14.5,
            "{:?}",
            p.position
        );
    }
}
