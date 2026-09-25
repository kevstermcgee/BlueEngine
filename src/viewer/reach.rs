//! Reachability analysis for BlueEngine maps.
//!
//! Evaluates where a player can physically walk starting from spawn or a specified location,
//! using the engine's exact collision capsules, step heights, and ground support logic.
//! Detects:
//! - Unreachable entities/rooms
//! - Perimeter leaks (player can walk off map boundary)
//! - Drop hazards (falls greater than DROP_THRESHOLD without railing)

use super::{
    authoring::MapDocument,
    controller::{CharacterKind, Collider, STANDING_HEIGHT},
};
use crate::math::V;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

pub const DROP_THRESHOLD: f32 = 0.60;
pub const STEP_HEIGHT: f32 = 0.22;
pub const CELL_SIZE: f32 = 0.25;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DropHazard {
    pub from: [f32; 3],
    pub to_y: f32,
    pub fall_distance: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReachReport {
    pub ok: bool,
    pub start: [f32; 3],
    pub reachable_cells: usize,
    pub bounding_min: [f32; 3],
    pub bounding_max: [f32; 3],
    pub unreachable_entities: Vec<String>,
    pub drop_hazards: Vec<DropHazard>,
    pub perimeter_leaks: Vec<[f32; 3]>,
}

/// Computes standable ground support height under position `(x, z)` for given feet height.
pub fn ground_support_at(
    x: f32,
    z: f32,
    current_feet: f32,
    radius: f32,
    colliders: &[Collider],
) -> Option<f32> {
    let p = V(x, 0., z);
    let mut support: Option<f32> = None;

    for c in colliders {
        if c.overlaps_xz(p, radius) {
            // Can step onto top of this collider if it's within reach (or we're falling onto it)
            if c.max.1 <= current_feet + STEP_HEIGHT + 0.01 && c.max.1 >= current_feet - 5.0 {
                if let Some(s) = support {
                    if c.max.1 >= s {
                        support = Some(c.max.1);
                    }
                } else {
                    support = Some(c.max.1);
                }
            }
        }
    }

    support
}

/// Checks if a player capsule with given radius and height at `(x, feet, z)` intersects any collider.
pub fn is_blocked(
    x: f32,
    feet: f32,
    z: f32,
    radius: f32,
    height: f32,
    colliders: &[Collider],
) -> bool {
    let p = V(x, feet + height * 0.5, z);
    colliders
        .iter()
        .any(|c| c.overlaps_body(p, feet, height, radius))
}

pub fn analyze_reach(doc: &MapDocument, start_pos: Option<V>) -> ReachReport {
    let radius = CharacterKind::Scientist.radius();
    let height = STANDING_HEIGHT;
    let colliders: Vec<Collider> = doc.colliders.values().cloned().collect();

    // Determine start position (spawn entity from map or default)
    let start = start_pos.unwrap_or_else(|| {
        if let Some(sp) = doc.entities.iter().find(|e| e.id.starts_with("spawn")) {
            V(
                (sp.bounds.min.0 + sp.bounds.max.0) * 0.5,
                sp.bounds.min.1,
                (sp.bounds.min.2 + sp.bounds.max.2) * 0.5,
            )
        } else {
            V(0., 0., 4.6)
        }
    });
    let start_feet =
        ground_support_at(start.0, start.2, start.1, radius, &colliders).unwrap_or(start.1);

    // Compute scene bounds
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    for c in &colliders {
        min_x = min_x.min(c.min.0);
        max_x = max_x.max(c.max.0);
        min_z = min_z.min(c.min.2);
        max_z = max_z.max(c.max.2);
    }
    for e in &doc.entities {
        min_x = min_x.min(e.bounds.min.0);
        max_x = max_x.max(e.bounds.max.0);
        min_z = min_z.min(e.bounds.min.2);
        max_z = max_z.max(e.bounds.max.2);
    }

    if min_x.is_infinite() {
        min_x = -20.0;
        max_x = 20.0;
        min_z = -20.0;
        max_z = 20.0;
    } else {
        min_x -= 2.0;
        max_x += 2.0;
        min_z -= 2.0;
        max_z += 2.0;
    }

    let mut visited: HashSet<(i32, i32, i32)> = HashSet::new(); // grid x, y_bucket (0.1m), grid z
    let mut queue: VecDeque<(f32, f32, f32)> = VecDeque::new();
    let mut drops: Vec<DropHazard> = Vec::new();
    let mut leaks: Vec<[f32; 3]> = Vec::new();

    let start_gx = (start.0 / CELL_SIZE).round() as i32;
    let start_gz = (start.2 / CELL_SIZE).round() as i32;
    let start_gy = (start_feet * 10.0).round() as i32;

    if !is_blocked(start.0, start_feet, start.2, radius, height, &colliders) {
        visited.insert((start_gx, start_gy, start_gz));
        queue.push_back((start.0, start_feet, start.2));
    }

    let dirs = [
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (0.707, 0.707),
        (-0.707, 0.707),
        (0.707, -0.707),
        (-0.707, -0.707),
    ];

    let mut bound_min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let mut bound_max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];

    let max_iterations = 50_000;
    let mut count = 0;

    while let Some((cx, cy, cz)) = queue.pop_front() {
        count += 1;
        if count > max_iterations {
            break;
        }

        bound_min[0] = bound_min[0].min(cx);
        bound_min[1] = bound_min[1].min(cy);
        bound_min[2] = bound_min[2].min(cz);
        bound_max[0] = bound_max[0].max(cx);
        bound_max[1] = bound_max[1].max(cy);
        bound_max[2] = bound_max[2].max(cz);

        // Check if touching outer perimeter
        if cx <= min_x + 0.5 || cx >= max_x - 0.5 || cz <= min_z + 0.5 || cz >= max_z - 0.5 {
            leaks.push([cx, cy, cz]);
        }

        for &(dx, dz) in &dirs {
            let nx = cx + dx * CELL_SIZE;
            let nz = cz + dz * CELL_SIZE;

            if nx < min_x || nx > max_x || nz < min_z || nz > max_z {
                continue;
            }

            if let Some(n_feet) = ground_support_at(nx, nz, cy, radius, &colliders) {
                let fall = cy - n_feet;
                if fall > DROP_THRESHOLD {
                    drops.push(DropHazard {
                        from: [cx, cy, cz],
                        to_y: n_feet,
                        fall_distance: fall,
                    });
                }

                if n_feet - cy <= STEP_HEIGHT
                    && !is_blocked(nx, n_feet, nz, radius, height, &colliders)
                {
                    let gx = (nx / CELL_SIZE).round() as i32;
                    let gz = (nz / CELL_SIZE).round() as i32;
                    let gy = (n_feet * 10.0).round() as i32;

                    if visited.insert((gx, gy, gz)) {
                        queue.push_back((nx, n_feet, nz));
                    }
                }
            }
        }
    }

    // Check entity reachability (within 2.5m of any visited point)
    let mut unreachable = Vec::new();
    for entity in &doc.entities {
        let center_x = (entity.bounds.min.0 + entity.bounds.max.0) * 0.5;
        let center_y = (entity.bounds.min.1 + entity.bounds.max.1) * 0.5;
        let center_z = (entity.bounds.min.2 + entity.bounds.max.2) * 0.5;

        let reached = visited.iter().any(|&(gx, gy, gz)| {
            let vx = gx as f32 * CELL_SIZE;
            let vy = gy as f32 * 0.1;
            let vz = gz as f32 * CELL_SIZE;
            let dx = center_x - vx;
            let dy = center_y - vy;
            let dz = center_z - vz;
            (dx * dx + dy * dy + dz * dz).sqrt() <= 2.5
        });

        if !reached {
            unreachable.push(entity.id.clone());
        }
    }

    // Dedup drops and leaks
    drops.sort_by(|a, b| a.from[0].partial_cmp(&b.from[0]).unwrap());
    drops.dedup_by(|a, b| {
        (a.from[0] - b.from[0]).abs() < 0.2 && (a.from[2] - b.from[2]).abs() < 0.2
    });

    leaks.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    leaks.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.3 && (a[2] - b[2]).abs() < 0.3);

    ReachReport {
        ok: unreachable.is_empty() && leaks.is_empty(),
        start: [start.0, start_feet, start.2],
        reachable_cells: visited.len(),
        bounding_min: bound_min,
        bounding_max: bound_max,
        unreachable_entities: unreachable,
        drop_hazards: drops,
        perimeter_leaks: leaks,
    }
}
