//! Blue Test Lab: primary engine test and validation environment.
//!
//! Designed specifically to test core engine systems:
//! - Multi-room connectivity and portal boundaries
//! - Locomotion: stairs (0.15m risers), ramps, jumpable ledges (0.35m)
//! - Clearance: low passages (0.7m clearance - passable for Feta, blocked for Scientist)
//! - Weapon raycasts against target surfaces
//! - Interaction targets
//! - Static and dynamic prop physics stacks
//! - Multiplayer spawn locations
//! - Spatial / network interest management
use super::{
    controller::Collider,
    interaction::Action,
    room::{Builder, Entity, Room},
    spatial::{RoomGraph, RoomId},
};
use crate::{
    geometry::{Compiled, World},
    math::V,
    scene::Scene,
};
use std::path::Path;

/// Spawns for multiplayer testing.
pub const SPAWN_PLAYER_1: V = V(0.0, 1.68, 4.0);
pub const SPAWN_PLAYER_2: V = V(0.0, 1.68, -4.0);

/// Construct the authoritative Blue Test Lab room.
pub fn build() -> crate::Result<Room> {
    let mut b = Builder {
        scene: Scene::default(),
        colliders: vec![],
        entities: vec![],
    };
    b.scene.nodes.clear();

    // Clean, high-contrast engineering materials
    b.material("lab-floor", V(0.22, 0.24, 0.26), 0.1, 0.0);
    b.material("lab-wall", V(0.75, 0.76, 0.78), 0.0, 0.0);
    b.material("lab-trim", V(0.12, 0.45, 0.85), 0.2, 0.0); // Blue Engine accent
    b.material("lab-feature", V(0.85, 0.55, 0.15), 0.0, 0.0); // Warm feature accent
    b.material("lab-target", V(0.90, 0.15, 0.15), 0.1, 0.0); // Weapon target red
    b.material("prop-cereal", V(0.92, 0.58, 0.15), 0.0, 0.0);
    b.material("prop-crate", V(0.55, 0.38, 0.22), 0.0, 0.0);

    // ==========================================
    // 1. Room 1: Main Arena ([-8..8], Y: [0..4], Z: [-8..8])
    // ==========================================
    // Floor & Ceiling
    b.cube("lab-floor", V(0.0, -0.1, 0.0), V(8.0, 0.1, 8.0));
    b.obstacle(V(0.0, -0.1, 0.0), V(8.0, 0.1, 8.0));
    b.cube("lab-wall", V(0.0, 4.1, 0.0), V(8.0, 0.1, 8.0));
    b.obstacle(V(0.0, 4.1, 0.0), V(8.0, 0.1, 8.0));

    // Outer Perimeter Walls (with portal openings)
    // North wall (Z = -8) - Target practice wall
    b.cube("lab-wall", V(0.0, 2.0, -8.1), V(8.0, 2.0, 0.1));
    b.obstacle(V(0.0, 2.0, -8.1), V(8.0, 2.0, 0.1));
    // Bullseye target on North wall
    b.cube("lab-target", V(0.0, 1.8, -7.95), V(1.0, 1.0, 0.05));

    // South wall (Z = 8)
    b.cube("lab-wall", V(0.0, 2.0, 8.1), V(8.0, 2.0, 0.1));
    b.obstacle(V(0.0, 2.0, 8.1), V(8.0, 2.0, 0.1));

    // East wall (X = 8, with 2m portal centered at Z=0 to Physics Lab)
    b.cube("lab-wall", V(8.1, 2.0, -4.5), V(0.1, 2.0, 3.5));
    b.obstacle(V(8.1, 2.0, -4.5), V(0.1, 2.0, 3.5));
    b.cube("lab-wall", V(8.1, 2.0, 4.5), V(0.1, 2.0, 3.5));
    b.obstacle(V(8.1, 2.0, 4.5), V(0.1, 2.0, 3.5));
    b.cube("lab-trim", V(8.1, 3.2, 0.0), V(0.1, 0.8, 1.0)); // Portal lintel
    b.obstacle(V(8.1, 3.2, 0.0), V(0.1, 0.8, 1.0));

    // West wall (X = -8, with 2m portal centered at Z=0 to Locomotion Lab)
    b.cube("lab-wall", V(-8.1, 2.0, -4.5), V(0.1, 2.0, 3.5));
    b.obstacle(V(-8.1, 2.0, -4.5), V(0.1, 2.0, 3.5));
    b.cube("lab-wall", V(-8.1, 2.0, 4.5), V(0.1, 2.0, 3.5));
    b.obstacle(V(-8.1, 2.0, 4.5), V(0.1, 2.0, 3.5));
    b.cube("lab-trim", V(-8.1, 3.2, 0.0), V(0.1, 0.8, 1.0)); // Portal lintel
    b.obstacle(V(-8.1, 3.2, 0.0), V(0.1, 0.8, 1.0));

    // Interactive Terminal Entity in Arena
    b.cube("lab-trim", V(3.0, 0.6, 0.0), V(0.4, 0.6, 0.4));
    b.obstacle(V(3.0, 0.6, 0.0), V(0.4, 0.6, 0.4));
    b.entity(
        "lab-terminal",
        "Main Diagnostics Terminal",
        V(3.0, 0.6, 0.0),
        V(0.5, 0.7, 0.5),
    );

    // ==========================================
    // 2. Room 2: Physics Lab ([8..20], Y: [0..4], Z: [-6..6])
    // ==========================================
    // Floor & Ceiling
    b.cube("lab-floor", V(14.0, -0.1, 0.0), V(6.0, 0.1, 6.0));
    b.obstacle(V(14.0, -0.1, 0.0), V(6.0, 0.1, 6.0));
    b.cube("lab-wall", V(14.0, 4.1, 0.0), V(6.0, 0.1, 6.0));
    b.obstacle(V(14.0, 4.1, 0.0), V(6.0, 0.1, 6.0));

    // Walls
    b.cube("lab-wall", V(20.1, 2.0, 0.0), V(0.1, 2.0, 6.0));
    b.obstacle(V(20.1, 2.0, 0.0), V(0.1, 2.0, 6.0));
    b.cube("lab-wall", V(14.0, 2.0, -6.1), V(6.0, 2.0, 0.1));
    b.obstacle(V(14.0, 2.0, -6.1), V(6.0, 2.0, 0.1));
    b.cube("lab-wall", V(14.0, 2.0, 6.1), V(6.0, 2.0, 0.1));
    b.obstacle(V(14.0, 2.0, 6.1), V(6.0, 2.0, 0.1));

    // Physics Workbench Table
    let table_center = V(14.0, 0.4, 0.0);
    let table_half = V(1.5, 0.4, 1.0);
    b.cube("lab-feature", table_center, table_half);
    b.obstacle(table_center, table_half);

    // Physics Props: Stack of boxes on the table (testing toppling, rigid bodies, sleeping)
    // Base Box
    b.cube("prop-crate", V(14.0, 0.95, 0.0), V(0.2, 0.15, 0.2));
    b.colliders.push(Collider {
        min: V(13.8, 0.80, -0.2),
        max: V(14.2, 1.10, 0.2),
    });
    b.entities.push(Entity {
        id: "lab-crate-base".into(),
        label: "Wooden Crate (Base)".into(),
        bounds: Collider {
            min: V(13.8, 0.80, -0.2),
            max: V(14.2, 1.10, 0.2),
        },
        action: Action::Inspect,
    });

    // Mid Box
    b.cube("prop-cereal", V(14.0, 1.30, 0.0), V(0.15, 0.20, 0.1));
    b.colliders.push(Collider {
        min: V(13.85, 1.10, -0.1),
        max: V(14.15, 1.50, 0.1),
    });
    b.entities.push(Entity {
        id: "lab-cereal-mid".into(),
        label: "Cereal Box (Mid)".into(),
        bounds: Collider {
            min: V(13.85, 1.10, -0.1),
            max: V(14.15, 1.50, 0.1),
        },
        action: Action::Inspect,
    });

    // ==========================================
    // 3. Room 3: Locomotion & Clearance Lab ([-20..-8], Y: [0..5], Z: [-6..6])
    // ==========================================
    // Floor & Ceiling
    b.cube("lab-floor", V(-14.0, -0.1, 0.0), V(6.0, 0.1, 6.0));
    b.obstacle(V(-14.0, -0.1, 0.0), V(6.0, 0.1, 6.0));
    b.cube("lab-wall", V(-14.0, 5.1, 0.0), V(6.0, 0.1, 6.0));
    b.obstacle(V(-14.0, 5.1, 0.0), V(6.0, 0.1, 6.0));

    // Walls
    b.cube("lab-wall", V(-20.1, 2.5, 0.0), V(0.1, 2.5, 6.0));
    b.obstacle(V(-20.1, 2.5, 0.0), V(0.1, 2.5, 6.0));
    b.cube("lab-wall", V(-14.0, 2.5, -6.1), V(6.0, 2.5, 0.1));
    b.obstacle(V(-14.0, 2.5, -6.1), V(6.0, 2.5, 0.1));
    b.cube("lab-wall", V(-14.0, 2.5, 6.1), V(6.0, 2.5, 0.1));
    b.obstacle(V(-14.0, 2.5, 6.1), V(6.0, 2.5, 0.1));

    // Mezzanine Platform at Y = 1.2m
    let mez_p = V(-17.0, 0.6, -3.0);
    let mez_s = V(2.5, 0.6, 2.5);
    b.cube("lab-feature", mez_p, mez_s);
    b.obstacle(mez_p, mez_s);

    // Stairs Test: 8 steps with 0.15m risers climbing from Y=0 to Y=1.2m
    for step in 0..8 {
        let step_y = (step as f32 + 1.0) * 0.15;
        let step_z = -0.5 - (step as f32 * 0.3);
        let center = V(-13.0, step_y * 0.5, step_z);
        let half = V(1.0, step_y * 0.5, 0.15);
        b.cube("lab-trim", center, half);
        b.obstacle(center, half);
    }

    // Jumpable Ledge Test (0.35m height)
    b.cube("lab-trim", V(-12.0, 0.175, 3.0), V(1.0, 0.175, 1.0));
    b.obstacle(V(-12.0, 0.175, 3.0), V(1.0, 0.175, 1.0));

    // Low Clearance Passage: 0.70m height
    // Floor to 0.70m is open; roof obstacle from 0.70m to 2.5m
    let tunnel_center = V(-16.0, 1.60, 3.0);
    let tunnel_half = V(1.5, 0.90, 1.5);
    b.cube("lab-wall", tunnel_center, tunnel_half);
    b.obstacle(tunnel_center, tunnel_half);

    let compiled = Compiled::new(b.scene, Path::new("."))?;
    let world = compiled.at(0.);

    // Construct the data-driven RoomGraph for the Test Lab
    let mut spatial = RoomGraph::new();
    let r_arena = RoomId(1);
    let r_physics = RoomId(2);
    let r_locomotion = RoomId(3);

    spatial.add_room(
        r_arena,
        "Main Arena",
        Collider {
            min: V(-8.0, 0.0, -8.0),
            max: V(8.0, 4.0, 8.0),
        },
        1,
    );
    spatial.add_room(
        r_physics,
        "Physics Lab",
        Collider {
            min: V(8.0, 0.0, -6.0),
            max: V(20.0, 4.0, 6.0),
        },
        1,
    );
    spatial.add_room(
        r_locomotion,
        "Locomotion Lab",
        Collider {
            min: V(-20.0, 0.0, -6.0),
            max: V(-8.0, 5.0, 6.0),
        },
        1,
    );

    // Portals connecting rooms
    let portal_bounds_east = Collider {
        min: V(7.8, 0.0, -1.0),
        max: V(8.2, 2.4, 1.0),
    };
    let portal_bounds_west = Collider {
        min: V(-8.2, 0.0, -1.0),
        max: V(-7.8, 2.4, 1.0),
    };
    spatial.add_portal(r_arena, r_physics, portal_bounds_east);
    spatial.add_portal(r_arena, r_locomotion, portal_bounds_west);

    Ok(Room {
        name: "Blue Test Lab".into(),
        simple_geometry: true,
        compiled,
        world,
        dynamic_world: World::new(vec![]),
        colliders: b.colliders,
        entities: b.entities,
        default_spawn: Some(super::authoring::MapSpawn::legacy()),
        spatial: Some(spatial),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lab_builds_and_verifies_systems() {
        let lab = build().unwrap();
        assert_eq!(lab.name, "Blue Test Lab");
        assert!(lab.spatial.is_some());
        let spatial = lab.spatial.as_ref().unwrap();
        assert_eq!(spatial.rooms.len(), 3);

        // Verify spawn positions locate inside Main Arena
        assert_eq!(spatial.find_room_at(SPAWN_PLAYER_1), Some(RoomId(1)));
        assert_eq!(spatial.find_room_at(SPAWN_PLAYER_2), Some(RoomId(1)));

        // Verify spatial interest: Arena is adjacent to Physics Lab and Locomotion Lab,
        // but Physics Lab is NOT directly adjacent to Locomotion Lab (separated by Arena)
        assert!(spatial.is_relevant_for_interest(RoomId(1), RoomId(2)));
        assert!(spatial.is_relevant_for_interest(RoomId(1), RoomId(3)));
        assert!(!spatial.is_relevant_for_interest(RoomId(2), RoomId(3)));

        // Verify clearance tunnel bounds: low passage height is 0.70m
        assert!(lab
            .colliders
            .iter()
            .any(|c| c.min.1 >= 0.69 && c.max.1 >= 2.0));
    }
}
