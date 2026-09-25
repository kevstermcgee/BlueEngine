//! Auto-planned walk tests, A* pathfinding, and blocking obstacle diagnostics.
//!
//! Generates physical routes between points or entities, simulates them with BlueEngine's
//! real 60 Hz physics `Controller`, and if blocked, identifies the exact offending colliders
//! and generates an explanatory SVG diagram.

use super::{
    authoring::MapDocument,
    controller::{CharacterKind, Collider, Controller, Movement, STANDING_HEIGHT},
    reach::{ground_support_at, is_blocked, CELL_SIZE, STEP_HEIGHT},
};
use crate::math::V;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Waypoint {
    pub x: f32,
    pub z: f32,
    pub feet: f32,
    #[serde(default)]
    pub crouch: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocker {
    pub id: String,
    pub gap: f32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub ahead: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkResult {
    pub ok: bool,
    pub ticks: usize,
    pub distance_m: f32,
    pub waypoints: Vec<Waypoint>,
    pub final_position: [f32; 3],
    pub final_feet: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blockers: Option<Vec<Blocker>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Copy, Clone, PartialEq)]
struct AStarNode {
    cost: f32,
    gx: i32,
    gz: i32,
    feet_bucket: i32,
}

impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn plan_route(doc: &MapDocument, from: V, to: V) -> Result<Vec<Waypoint>, String> {
    let radius = CharacterKind::Scientist.radius() + 0.05; // slightly wider for path clearance
    let height = STANDING_HEIGHT;
    let colliders: Vec<Collider> = doc.colliders.values().cloned().collect();

    let start_feet =
        ground_support_at(from.0, from.2, from.1, radius, &colliders).unwrap_or(from.1);
    let target_feet = ground_support_at(to.0, to.2, to.1, radius, &colliders).unwrap_or(to.1);

    if is_blocked(from.0, start_feet, from.2, radius, height, &colliders) {
        return Err("Start position is inside a collider".into());
    }

    let start_gx = (from.0 / CELL_SIZE).round() as i32;
    let start_gz = (from.2 / CELL_SIZE).round() as i32;
    let start_gy = (start_feet * 10.0).round() as i32;

    let target_gx = (to.0 / CELL_SIZE).round() as i32;
    let target_gz = (to.2 / CELL_SIZE).round() as i32;

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<(i32, i32, i32), f32> = HashMap::new();
    let mut came_from: HashMap<(i32, i32, i32), (i32, i32, i32, f32)> = HashMap::new(); // key -> (prev, feet)

    let h_start = ((from.0 - to.0).powi(2) + (from.2 - to.2).powi(2)).sqrt();
    g_score.insert((start_gx, start_gy, start_gz), 0.0);
    open.push(AStarNode {
        cost: h_start,
        gx: start_gx,
        gz: start_gz,
        feet_bucket: start_gy,
    });

    let dirs = [
        (1, 0, 1.0_f32),
        (-1, 0, 1.0),
        (0, 1, 1.0),
        (0, -1, 1.0),
        (1, 1, 1.414),
        (-1, 1, 1.414),
        (1, -1, 1.414),
        (-1, -1, 1.414),
    ];

    let mut reached_key = None;
    let max_iterations = 25_000;
    let mut iterations = 0;

    while let Some(current) = open.pop() {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        if (current.gx - target_gx).abs() <= 1 && (current.gz - target_gz).abs() <= 1 {
            reached_key = Some((current.gx, current.feet_bucket, current.gz));
            break;
        }

        let curr_g = *g_score
            .get(&(current.gx, current.feet_bucket, current.gz))
            .unwrap_or(&f32::INFINITY);
        let _curr_x = current.gx as f32 * CELL_SIZE;
        let _curr_z = current.gz as f32 * CELL_SIZE;
        let curr_feet = current.feet_bucket as f32 * 0.1;

        for &(dx, dz, dist_weight) in &dirs {
            let ngx = current.gx + dx;
            let ngz = current.gz + dz;
            let nx = ngx as f32 * CELL_SIZE;
            let nz = ngz as f32 * CELL_SIZE;

            if let Some(n_feet) = ground_support_at(nx, nz, curr_feet, radius, &colliders) {
                if n_feet - curr_feet <= STEP_HEIGHT {
                    if !is_blocked(nx, n_feet, nz, radius, height, &colliders) {
                        let n_gy = (n_feet * 10.0).round() as i32;
                        let tentative_g =
                            curr_g + dist_weight * CELL_SIZE + (n_feet - curr_feet).abs() * 0.5;
                        let neighbor_key = (ngx, n_gy, ngz);

                        if tentative_g < *g_score.get(&neighbor_key).unwrap_or(&f32::INFINITY) {
                            g_score.insert(neighbor_key, tentative_g);
                            came_from.insert(
                                neighbor_key,
                                (current.gx, current.feet_bucket, current.gz, curr_feet),
                            );
                            let h = ((nx - to.0).powi(2) + (nz - to.2).powi(2)).sqrt();
                            open.push(AStarNode {
                                cost: tentative_g + h,
                                gx: ngx,
                                gz: ngz,
                                feet_bucket: n_gy,
                            });
                        }
                    }
                }
            }
        }
    }

    let Some(mut curr) = reached_key else {
        return Err(format!(
            "No walkable route found from ({:.2}, {:.2}) to ({:.2}, {:.2})",
            from.0, from.2, to.0, to.2
        ));
    };

    let mut raw_path = Vec::new();
    raw_path.push(Waypoint {
        x: to.0,
        z: to.2,
        feet: target_feet,
        crouch: false,
    });

    while let Some(&(px, py, pz, feet)) = came_from.get(&curr) {
        raw_path.push(Waypoint {
            x: curr.0 as f32 * CELL_SIZE,
            z: curr.2 as f32 * CELL_SIZE,
            feet,
            crouch: false,
        });
        curr = (px, py, pz);
    }

    raw_path.push(Waypoint {
        x: from.0,
        z: from.2,
        feet: start_feet,
        crouch: false,
    });

    raw_path.reverse();

    // String-pulling waypoint simplification
    let mut simplified = Vec::new();
    if raw_path.is_empty() {
        return Ok(simplified);
    }

    simplified.push(raw_path[0]);
    let mut i = 0;
    while i < raw_path.len() - 1 {
        let mut furthest = i + 1;
        for j in (i + 2)..raw_path.len() {
            // Check direct line of sight from raw_path[i] to raw_path[j]
            let a = V(raw_path[i].x, raw_path[i].feet, raw_path[i].z);
            let b = V(raw_path[j].x, raw_path[j].feet, raw_path[j].z);
            let dist = (a - b).length();
            let steps = (dist / (CELL_SIZE * 0.5)).ceil() as usize;
            let mut clear = true;

            for s in 1..steps {
                let t = s as f32 / steps as f32;
                let test_pos = a.lerp(b, t);
                if is_blocked(
                    test_pos.0, test_pos.1, test_pos.2, radius, height, &colliders,
                ) {
                    clear = false;
                    break;
                }
            }

            if clear {
                furthest = j;
            } else {
                break;
            }
        }
        simplified.push(raw_path[furthest]);
        i = furthest;
    }

    Ok(simplified)
}

pub fn execute_walk(
    doc: &MapDocument,
    from: V,
    to: V,
    waypoints: Option<Vec<Waypoint>>,
) -> WalkResult {
    let colliders: Vec<Collider> = doc.colliders.values().cloned().collect();
    let waypoints = match waypoints {
        Some(w) => w,
        None => match plan_route(doc, from, to) {
            Ok(w) => w,
            Err(e) => {
                return WalkResult {
                    ok: false,
                    ticks: 0,
                    distance_m: 0.0,
                    waypoints: vec![],
                    final_position: [from.0, from.1, from.2],
                    final_feet: from.1,
                    blockers: None,
                    explanation: Some(e),
                };
            }
        },
    };

    let mut controller = Controller::for_profile(Default::default(), from, 0.0).unwrap_or_default();
    let mut total_ticks = 0;
    let mut total_distance = 0.0;
    let mut prev_pos = controller.position;

    for (idx, wp) in waypoints.iter().enumerate() {
        let mut reached = false;
        let max_steps_for_leg = 1200; // 20 seconds at 60 Hz
        let mut step = 0;

        while step < max_steps_for_leg {
            step += 1;
            total_ticks += 1;

            let delta = V(
                wp.x - controller.position.0,
                0.,
                wp.z - controller.position.2,
            );
            let dist_xz = delta.length();
            let feet_diff = (controller.feet_height() - wp.feet).abs();

            if dist_xz < 0.12 && feet_diff < 0.15 {
                controller.stop();
                reached = true;
                break;
            }

            controller.yaw = delta.0.atan2(-delta.2);
            controller.update(
                Movement {
                    forward: 1.0,
                    crouch: wp.crouch,
                    ..Default::default()
                },
                1.0 / 60.0,
                &colliders,
            );

            let d = (controller.position - prev_pos).length();
            total_distance += d;
            prev_pos = controller.position;
        }

        if !reached {
            // Diagnose blockage
            let blockers = diagnose_blockers(doc, controller.position, V(wp.x, wp.feet, wp.z));
            let explanation = format!(
                "Walk blocked on way to waypoint {idx} ({:.2}, {:.2}): player stopped at ({:.2}, {:.2}, {:.2}). Blocked by {} colliders.",
                wp.x, wp.z, controller.position.0, controller.position.1, controller.position.2, blockers.len()
            );

            return WalkResult {
                ok: false,
                ticks: total_ticks,
                distance_m: total_distance,
                waypoints,
                final_position: [
                    controller.position.0,
                    controller.position.1,
                    controller.position.2,
                ],
                final_feet: controller.feet_height(),
                blockers: Some(blockers),
                explanation: Some(explanation),
            };
        }
    }

    WalkResult {
        ok: true,
        ticks: total_ticks,
        distance_m: total_distance,
        waypoints,
        final_position: [
            controller.position.0,
            controller.position.1,
            controller.position.2,
        ],
        final_feet: controller.feet_height(),
        blockers: None,
        explanation: None,
    }
}

pub fn diagnose_blockers(doc: &MapDocument, player_pos: V, target_pos: V) -> Vec<Blocker> {
    let radius = CharacterKind::Scientist.radius();
    let forward = (target_pos - player_pos).norm();
    let mut blockers = Vec::new();

    for (id, c) in &doc.colliders {
        let center = V(
            (c.min.0 + c.max.0) * 0.5,
            (c.min.1 + c.max.1) * 0.5,
            (c.min.2 + c.max.2) * 0.5,
        );
        let closest_x = player_pos.0.clamp(c.min.0, c.max.0);
        let closest_z = player_pos.2.clamp(c.min.2, c.max.2);
        let gap = ((player_pos.0 - closest_x).powi(2) + (player_pos.2 - closest_z).powi(2)).sqrt()
            - radius;

        if gap <= 0.12 && c.max.1 > player_pos.1 - 1.5 && c.min.1 < player_pos.1 + 0.5 {
            let to_blocker = (center - player_pos).norm();
            let ahead = forward.0 * to_blocker.0 + forward.2 * to_blocker.2 > 0.0;

            blockers.push(Blocker {
                id: id.clone(),
                gap: gap.max(0.0),
                bounds_min: [c.min.0, c.min.1, c.min.2],
                bounds_max: [c.max.0, c.max.1, c.max.2],
                ahead,
            });
        }
    }

    blockers.sort_by(|a, b| a.gap.partial_cmp(&b.gap).unwrap_or(Ordering::Equal));
    blockers
}

pub fn generate_blocker_svg(
    doc: &MapDocument,
    player_pos: [f32; 3],
    target_pos: [f32; 3],
    blockers: &[Blocker],
) -> String {
    let px = player_pos[0];
    let pz = player_pos[2];
    let tx = target_pos[0];
    let tz = target_pos[2];

    let view_width = 8.0_f32; // 8x8m window around player
    let scale = 80.0_f32; // 80px per meter
    let svg_size = (view_width * scale) as u32;

    let to_svg = |x: f32, z: f32| -> (f32, f32) {
        (
            svg_size as f32 * 0.5 + (x - px) * scale,
            svg_size as f32 * 0.5 + (z - pz) * scale,
        )
    };

    let blocker_ids: HashSet<&str> = blockers.iter().map(|b| b.id.as_str()).collect();

    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {svg_size} {svg_size}'>\n\
         <rect width='{svg_size}' height='{svg_size}' fill='#1a1f26'/>\n\
         <style>\n\
           .grid {{ stroke: #2d3748; stroke-width: 0.5; }}\n\
           .collider {{ fill: #4a5568; fill-opacity: 0.7; stroke: #718096; stroke-width: 1.5; }}\n\
           .blocker {{ fill: #e53e3e; fill-opacity: 0.8; stroke: #fc8181; stroke-width: 2.5; }}\n\
           .player {{ fill: #3182ce; stroke: #63b3ed; stroke-width: 2; }}\n\
           .target-line {{ stroke: #ecc94b; stroke-width: 2; stroke-dasharray: 4; }}\n\
           text {{ font-family: monospace; font-size: 11px; fill: #e2e8f0; }}\n\
         </style>\n"
    );

    // Draw colliders in view
    for (id, c) in &doc.colliders {
        let (x1, y1) = to_svg(c.min.0, c.min.2);
        let (x2, y2) = to_svg(c.max.0, c.max.2);
        let width = (x2 - x1).abs();
        let height = (y2 - y1).abs();
        let rx = x1.min(x2);
        let ry = y1.min(y2);

        let is_blocker = blocker_ids.contains(id.as_str());
        let class = if is_blocker { "blocker" } else { "collider" };

        svg.push_str(&format!(
            "  <rect class='{class}' x='{rx:.1}' y='{ry:.1}' width='{width:.1}' height='{height:.1}'>\n\
             <title>{id}</title>\n\
             </rect>\n"
        ));
        if is_blocker {
            svg.push_str(&format!(
                "  <text x='{:.1}' y='{:.1}'>BLOCKED: {}</text>\n",
                rx + 4.0,
                ry + 14.0,
                id
            ));
        }
    }

    // Player position
    let (center_x, center_y) = to_svg(px, pz);
    let player_r = CharacterKind::Scientist.radius() * scale;
    svg.push_str(&format!(
        "  <circle class='player' cx='{center_x:.1}' cy='{center_y:.1}' r='{player_r:.1}'/>\n\
         <text x='{:.1}' y='{:.1}'>PLAYER</text>\n",
        center_x - 18.0,
        center_y - player_r - 4.0
    ));

    // Target heading arrow
    let (target_svg_x, target_svg_y) = to_svg(tx, tz);
    svg.push_str(&format!(
        "  <line class='target-line' x1='{center_x:.1}' y1='{center_y:.1}' x2='{target_svg_x:.1}' y2='{target_svg_y:.1}'/>\n"
    ));

    svg.push_str("</svg>\n");
    svg
}
