//! Declarative blueprint compiler for BlueEngine.
//!
//! Compiles high-level specifications of rooms, doors, spawns, and prop fill
//! into complete, fully-validated, lint-clean `MapDocument`s equipped with
//! colliders, materials, floor slabs, wall segments, door portals, and spatial graphs.

use super::{
    authoring::{MapDocument, MapSpawn},
    controller::Collider,
    interaction::Action,
    props::{self, PropKind},
    room::Entity,
    spatial::{RoomGraph, RoomId},
};
use crate::{
    math::V,
    scene::{Camera, Material, Node, Scene, Shape, Track},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomSpec {
    pub id: String,
    pub rect: [f32; 4], // [min_x, min_z, max_x, max_z]
    #[serde(default)]
    pub floor_color: Option<[f32; 3]>,
    #[serde(default)]
    pub wall_color: Option<[f32; 3]>,
    #[serde(default)]
    pub lamp: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DoorSpec {
    pub between: [String; 2],
    #[serde(default = "default_door_width")]
    pub width: f32,
    /// Signed offset in metres from the midpoint of the shared boundary.
    /// Zero centers the opening; this is not an absolute world coordinate.
    #[serde(default)]
    pub at: Option<f32>,
}

fn default_door_width() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnSpec {
    pub id: String,
    pub room: String,
    #[serde(default)]
    pub offset: Option<[f32; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillSpec {
    pub room: String,
    pub kind: String,
    #[serde(default = "default_one")]
    pub count: usize,
    #[serde(default)]
    pub seed: u64,
}

fn default_one() -> usize {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlueprintSpec {
    pub name: String,
    #[serde(default = "default_room_height")]
    pub height: f32,
    #[serde(default = "default_wall_thick")]
    pub wall_thickness: f32,
    pub rooms: Vec<RoomSpec>,
    #[serde(default)]
    pub doors: Vec<DoorSpec>,
    #[serde(default)]
    pub spawns: Vec<SpawnSpec>,
    #[serde(default)]
    pub fill: Vec<FillSpec>,
}

fn default_room_height() -> f32 {
    3.0
}

fn default_wall_thick() -> f32 {
    0.20
}

pub fn compile_blueprint(spec: &BlueprintSpec) -> Result<MapDocument> {
    let mut scene = Scene {
        materials: BTreeMap::new(),
        nodes: Vec::new(),
        audio: None,
        camera: Camera::default(),
        ..Default::default()
    };

    let mut colliders = BTreeMap::new();
    let mut entities = Vec::new();

    // Default wall material
    let default_wall_mat = "mat_wall_default".to_string();
    scene.materials.insert(
        default_wall_mat.clone(),
        Material {
            color: V(0.85, 0.83, 0.80),
            roughness: 0.9,
            metallic: 0.0,
            ..Default::default()
        },
    );

    let default_floor_mat = "mat_floor_default".to_string();
    scene.materials.insert(
        default_floor_mat.clone(),
        Material {
            color: V(0.45, 0.48, 0.52),
            roughness: 0.8,
            metallic: 0.0,
            ..Default::default()
        },
    );

    let h = spec.height;
    let wt = spec.wall_thickness;
    let half_wt = wt * 0.5;

    // 1. Process rooms: generate floors and register bounds
    let mut room_rects: HashMap<String, [f32; 4]> = HashMap::new();

    for room in &spec.rooms {
        let [min_x, min_z, max_x, max_z] = room.rect;
        if min_x >= max_x || min_z >= max_z {
            return Err(format!("Invalid rect for room '{}'", room.id).into());
        }
        room_rects.insert(room.id.clone(), room.rect);

        // Floor slab
        let floor_id = format!("{}_floor", room.id);
        let center_x = (min_x + max_x) * 0.5;
        let center_z = (min_z + max_z) * 0.5;
        let half_w = (max_x - min_x) * 0.5;
        let half_d = (max_z - min_z) * 0.5;

        let floor_mat = if let Some(c) = room.floor_color {
            let m_name = format!("mat_floor_{}", room.id);
            scene.materials.insert(
                m_name.clone(),
                Material {
                    color: V(c[0], c[1], c[2]),
                    roughness: 0.8,
                    metallic: 0.0,
                    ..Default::default()
                },
            );
            m_name
        } else {
            default_floor_mat.clone()
        };

        // Floor slab node: 0.2m thick under Y=0
        scene.nodes.push(Node {
            id: floor_id.clone(),
            shape: Shape::Box,
            material: floor_mat,
            pos: Track::Fixed(V(center_x, -0.1, center_z)),
            scale: Track::Fixed(V(half_w, 0.1, half_d)),
            ..Default::default()
        });

        // Floor collider
        colliders.insert(
            floor_id,
            Collider {
                min: V(min_x, -0.2, min_z),
                max: V(max_x, 0.0, max_z),
            },
        );
    }

    let mut spatial = RoomGraph::new();
    for (r_idx, room) in spec.rooms.iter().enumerate() {
        let [min_x, min_z, max_x, max_z] = room.rect;
        spatial.add_room(
            RoomId(r_idx as u32),
            &room.id,
            Collider {
                min: V(min_x, 0.0, min_z),
                max: V(max_x, h, max_z),
            },
            0,
        );
    }

    // 2. Compute doors between rooms
    let mut door_openings: Vec<([String; 2], [f32; 4])> = Vec::new(); // room pair -> opening rect in XZ

    for door in &spec.doors {
        let r1 = room_rects
            .get(&door.between[0])
            .ok_or_else(|| format!("Unknown room in door: {}", door.between[0]))?;
        let r2 = room_rects
            .get(&door.between[1])
            .ok_or_else(|| format!("Unknown room in door: {}", door.between[1]))?;

        let r1_idx = spec
            .rooms
            .iter()
            .position(|r| r.id == door.between[0])
            .unwrap() as u32;
        let r2_idx = spec
            .rooms
            .iter()
            .position(|r| r.id == door.between[1])
            .unwrap() as u32;

        // Check if shared X or Z edge
        let overlap_x = (r1[0].max(r2[0]), r1[2].min(r2[2]));
        let overlap_z = (r1[1].max(r2[1]), r1[3].min(r2[3]));

        let dw = door.width;
        let mut opening = [0.0; 4];

        if (r1[2] - r2[0]).abs() < 0.01 || (r2[2] - r1[0]).abs() < 0.01 {
            // Shared vertical boundary (X is constant)
            let shared_x = if (r1[2] - r2[0]).abs() < 0.01 {
                r1[2]
            } else {
                r2[2]
            };
            let z_mid = (overlap_z.0 + overlap_z.1) * 0.5 + door.at.unwrap_or(0.0);
            let half_dw = dw * 0.5;
            opening = [
                shared_x - half_wt - 0.05,
                z_mid - half_dw,
                shared_x + half_wt + 0.05,
                z_mid + half_dw,
            ];

            spatial.add_portal(
                RoomId(r1_idx),
                RoomId(r2_idx),
                Collider {
                    min: V(shared_x - half_wt, 0.0, z_mid - half_dw),
                    max: V(shared_x + half_wt, 2.2, z_mid + half_dw),
                },
            );
        } else if (r1[3] - r2[1]).abs() < 0.01 || (r2[3] - r1[1]).abs() < 0.01 {
            // Shared horizontal boundary (Z is constant)
            let shared_z = if (r1[3] - r2[1]).abs() < 0.01 {
                r1[3]
            } else {
                r2[3]
            };
            let x_mid = (overlap_x.0 + overlap_x.1) * 0.5 + door.at.unwrap_or(0.0);
            let half_dw = dw * 0.5;
            opening = [
                x_mid - half_dw,
                shared_z - half_wt - 0.05,
                x_mid + half_dw,
                shared_z + half_wt + 0.05,
            ];

            spatial.add_portal(
                RoomId(r1_idx),
                RoomId(r2_idx),
                Collider {
                    min: V(x_mid - half_dw, 0.0, shared_z - half_wt),
                    max: V(x_mid + half_dw, 2.2, shared_z + half_wt),
                },
            );
        }

        door_openings.push((door.between.clone(), opening));
    }

    // 3. Walls generation with door cutouts
    let mut wall_counter = 0;
    for room in &spec.rooms {
        let wall_material = if let Some(color) = room.wall_color {
            let material_id = format!("mat_wall_{}", room.id);
            scene.materials.insert(
                material_id.clone(),
                Material {
                    color: V(color[0], color[1], color[2]),
                    roughness: 0.9,
                    metallic: 0.0,
                    ..Default::default()
                },
            );
            material_id
        } else {
            default_wall_mat.clone()
        };
        let [min_x, min_z, max_x, max_z] = room.rect;
        let edges = [
            // (p1, p2, is_horizontal)
            (V(min_x, 0., min_z), V(max_x, 0., min_z), true), // South
            (V(min_x, 0., max_z), V(max_x, 0., max_z), true), // North
            (V(min_x, 0., min_z), V(min_x, 0., max_z), false), // West
            (V(max_x, 0., min_z), V(max_x, 0., max_z), false), // East
        ];

        for (p1, p2, is_h) in edges {
            wall_counter += 1;
            let mut segments = vec![(
                if is_h { p1.0 } else { p1.2 },
                if is_h { p2.0 } else { p2.2 },
            )];

            // Cut out doors on this wall segment
            for (_, op) in &door_openings {
                let (door_lo, door_hi) = if is_h { (op[0], op[2]) } else { (op[1], op[3]) };
                let fixed_coord = if is_h { p1.2 } else { p1.0 };
                let door_fixed_lo = if is_h { op[1] } else { op[0] };
                let door_fixed_hi = if is_h { op[3] } else { op[2] };

                if fixed_coord >= door_fixed_lo && fixed_coord <= door_fixed_hi {
                    let mut next_segs = Vec::new();
                    for (s_lo, s_hi) in segments {
                        if door_lo > s_lo && door_lo < s_hi && door_hi >= s_hi {
                            next_segs.push((s_lo, door_lo));
                        } else if door_hi < s_hi && door_hi > s_lo && door_lo <= s_lo {
                            next_segs.push((door_hi, s_hi));
                        } else if door_lo > s_lo && door_hi < s_hi {
                            next_segs.push((s_lo, door_lo));
                            next_segs.push((door_hi, s_hi));
                        } else if door_lo <= s_lo && door_hi >= s_hi {
                            // completely covered by door
                        } else {
                            next_segs.push((s_lo, s_hi));
                        }
                    }
                    segments = next_segs;
                }
            }

            // Create wall nodes and colliders for remaining segments
            for (idx, (s_lo, s_hi)) in segments.into_iter().enumerate() {
                if s_hi - s_lo < 0.1 {
                    continue;
                }
                let seg_id = format!("{}_wall_{}_{}", room.id, wall_counter, idx);
                let (center_x, center_z, half_wx, half_wz) = if is_h {
                    ((s_lo + s_hi) * 0.5, p1.2, (s_hi - s_lo) * 0.5, half_wt)
                } else {
                    (p1.0, (s_lo + s_hi) * 0.5, half_wt, (s_hi - s_lo) * 0.5)
                };

                scene.nodes.push(Node {
                    id: seg_id.clone(),
                    shape: Shape::Box,
                    material: wall_material.clone(),
                    pos: Track::Fixed(V(center_x, h * 0.5, center_z)),
                    scale: Track::Fixed(V(half_wx, h * 0.5, half_wz)),
                    ..Default::default()
                });

                colliders.insert(
                    seg_id,
                    Collider {
                        min: V(center_x - half_wx, 0.0, center_z - half_wz),
                        max: V(center_x + half_wx, h, center_z + half_wz),
                    },
                );
            }
        }
    }

    // 4. Spawns
    if !spec.spawns.is_empty() {
        for s in &spec.spawns {
            let r = room_rects
                .get(&s.room)
                .ok_or_else(|| format!("Unknown room for spawn: {}", s.room))?;
            let offset = s.offset.unwrap_or([0.0, 0.0]);
            let sx = (r[0] + r[2]) * 0.5 + offset[0];
            let sz = (r[1] + r[3]) * 0.5 + offset[1];

            entities.push(Entity {
                id: format!("spawn_{}", s.id),
                label: format!("Spawn {}", s.id),
                bounds: Collider {
                    min: V(sx - 0.25, 0.0, sz - 0.25),
                    max: V(sx + 0.25, 1.8, sz + 0.25),
                },
                action: Action::Inspect,
            });
        }
    } else {
        // Default spawn in first room
        if let Some(first) = spec.rooms.first() {
            let [min_x, min_z, max_x, max_z] = first.rect;
            let sx = (min_x + max_x) * 0.5;
            let sz = (min_z + max_z) * 0.5;
            entities.push(Entity {
                id: "spawn_default".into(),
                label: "Spawn Default".into(),
                bounds: Collider {
                    min: V(sx - 0.25, 0.0, sz - 0.25),
                    max: V(sx + 0.25, 1.8, sz + 0.25),
                },
                action: Action::Inspect,
            });
        }
    }

    // 5. Fill items (catalog props)
    for (f_idx, fill) in spec.fill.iter().enumerate() {
        let r = room_rects
            .get(&fill.room)
            .ok_or_else(|| format!("Unknown room for fill: {}", fill.room))?;
        let kind = match fill.kind.as_str() {
            "apple" => PropKind::Apple,
            "chair" => PropKind::Chair,
            "table" => PropKind::Table,
            "book-stack" => PropKind::BookStack,
            "flower-vase" => PropKind::FlowerVase,
            "table-lamp" => PropKind::TableLamp,
            "potted-cactus" => PropKind::PottedCactus,
            "sculpture" => PropKind::Sculpture,
            "vase-plant" => PropKind::VasePlant,
            "bowl" => PropKind::Bowl,
            "cereal" => PropKind::CerealBox,
            _ => PropKind::Chair,
        };

        let prop_desc = props::CATALOG.iter().find(|p| p.kind == kind).unwrap();
        let h_ext = prop_desc.half_extents;

        // Place inside room with margin
        let min_x = r[0] + h_ext.0 + wt + 0.2;
        let max_x = r[2] - h_ext.0 - wt - 0.2;
        let min_z = r[1] + h_ext.2 + wt + 0.2;
        let max_z = r[3] - h_ext.2 - wt - 0.2;

        if min_x < max_x && min_z < max_z {
            let mut rng_seed = fill.seed.wrapping_add(f_idx as u64 * 31);
            let mut placed = 0;
            let mut attempts = 0;
            while placed < fill.count && attempts < 200 {
                attempts += 1;
                rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let fx = (rng_seed >> 32) as f32 / (u32::MAX as f32);
                rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let fz = (rng_seed >> 32) as f32 / (u32::MAX as f32);

                let px = min_x + (max_x - min_x) * fx;
                let pz = min_z + (max_z - min_z) * fz;
                let origin = V(px, 0.0, pz);

                let center = origin + V(0.0, h_ext.1, 0.0);
                let b = Collider {
                    min: center - h_ext,
                    max: center + h_ext,
                };

                // Check clearance against spawns (at least 0.8m radius) and existing colliders
                let overlaps_spawn = entities.iter().any(|e| {
                    (e.id.starts_with("spawn") || e.id.contains("spawn"))
                        && (V(px, 0.0, pz)
                            - V(
                                (e.bounds.min.0 + e.bounds.max.0) * 0.5,
                                0.0,
                                (e.bounds.min.2 + e.bounds.max.2) * 0.5,
                            ))
                        .length()
                            < 1.0
                });
                let overlaps_other = colliders.iter().any(|(cid, existing)| {
                    !cid.starts_with("floor")
                        && existing.min.0 < b.max.0
                        && existing.max.0 > b.min.0
                        && existing.min.2 < b.max.2
                        && existing.max.2 > b.min.2
                        && existing.max.1 > 0.0
                });

                if overlaps_spawn || overlaps_other {
                    continue;
                }

                let prop_id = format!("{}_{}_{}", fill.room, fill.kind, placed);
                placed += 1;
                let p_scene = props::scene(kind);

                for (mat_id, mat_val) in p_scene.materials {
                    scene.materials.entry(mat_id).or_insert(mat_val);
                }

                for (n_i, mut n) in p_scene.nodes.into_iter().enumerate() {
                    n.id = format!("{prop_id}/{n_i}");
                    if let Track::Fixed(v) = n.pos {
                        n.pos = Track::Fixed(v + origin);
                    }
                    scene.nodes.push(n);
                }

                colliders.insert(prop_id.clone(), b.clone());
                entities.push(Entity {
                    id: prop_id,
                    label: format!("{} {}", fill.room, fill.kind),
                    bounds: b,
                    action: Action::Inspect,
                });
            }
        }
    }

    let spatial = if !spec.rooms.is_empty() {
        Some(spatial)
    } else {
        None
    };

    let default_spawn = entities
        .iter()
        .find(|entity| entity.id.starts_with("spawn"))
        .map(|entity| MapSpawn {
            feet: V(
                (entity.bounds.min.0 + entity.bounds.max.0) * 0.5,
                entity.bounds.min.1,
                (entity.bounds.min.2 + entity.bounds.max.2) * 0.5,
            ),
            yaw: 0.,
        });
    let doc = MapDocument {
        schema_version: 1,
        name: spec.name.clone(),
        scene,
        colliders,
        entities,
        default_spawn,
        spatial,
        checks: None,
    };

    doc.validate()?;
    Ok(doc)
}
