//! Procedural placement generators for BlueEngine: `scatter` and `line`.
//!
//! Places concrete props from the catalog into maps with deterministic random seeding,
//! ground snapping, and collision avoidance (preventing overlaps with walls or furniture).

use super::{
    authoring::MapDocument,
    controller::Collider,
    interaction::Action,
    props::{self, PropKind},
    reach::ground_support_at,
    room::Entity,
};
use crate::{math::V, scene::Track, Result};

pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn parse_prop_kind(name: &str) -> Result<PropKind> {
    match name {
        "apple" => Ok(PropKind::Apple),
        "chair" => Ok(PropKind::Chair),
        "table" => Ok(PropKind::Table),
        "book-stack" => Ok(PropKind::BookStack),
        "flower-vase" => Ok(PropKind::FlowerVase),
        "tall-vase" => Ok(PropKind::TallVase),
        "table-lamp" => Ok(PropKind::TableLamp),
        "candle-trio" => Ok(PropKind::CandleTrio),
        "potted-cactus" => Ok(PropKind::PottedCactus),
        "mantel-clock" => Ok(PropKind::MantelClock),
        "woven-basket" => Ok(PropKind::WovenBasket),
        "framed-art" => Ok(PropKind::FramedArt),
        "framed-botanical" => Ok(PropKind::FramedBotanical),
        "sculpture" => Ok(PropKind::Sculpture),
        "vase-plant" => Ok(PropKind::VasePlant),
        "bowl" => Ok(PropKind::Bowl),
        "cereal" => Ok(PropKind::CerealBox),
        _ => Err(
            format!("Unknown prop kind '{name}'. Use 'catalog' to list available props.").into(),
        ),
    }
}

pub fn scatter(
    mut doc: MapDocument,
    kind_name: &str,
    count: usize,
    rect: [f32; 4], // min_x, min_z, max_x, max_z
    seed: u64,
    prefix: Option<&str>,
) -> Result<(MapDocument, usize)> {
    let kind = parse_prop_kind(kind_name)?;
    let p_desc = props::CATALOG.iter().find(|d| d.kind == kind).unwrap();
    let h_ext = p_desc.half_extents;
    let radius = h_ext.0.max(h_ext.2);

    let [min_x, min_z, max_x, max_z] = rect;
    if min_x >= max_x || min_z >= max_z {
        return Err("Invalid scatter bounding rect".into());
    }

    let mut rng = SplitMix64::new(seed);
    let mut placed = 0;
    let existing_colliders: Vec<Collider> = doc.colliders.values().cloned().collect();
    let p_scene = props::scene(kind);
    let prefix = prefix.unwrap_or(kind_name);

    for (mat_id, mat_val) in &p_scene.materials {
        doc.scene
            .materials
            .entry(mat_id.clone())
            .or_insert_with(|| mat_val.clone());
    }

    let max_attempts = count * 20;
    let mut attempts = 0;

    while placed < count && attempts < max_attempts {
        attempts += 1;
        let rx = min_x + (max_x - min_x) * rng.f32();
        let rz = min_z + (max_z - min_z) * rng.f32();

        let feet_y = ground_support_at(rx, rz, 0.0, radius, &existing_colliders).unwrap_or(0.0);
        let origin = V(rx, feet_y, rz);
        let center = origin + V(0.0, h_ext.1, 0.0);
        let candidate_bounds = Collider {
            min: center - h_ext,
            max: center + h_ext,
        };

        // Check clearance with existing colliders
        let collides = existing_colliders
            .iter()
            .any(|c| c.overlaps_body(V(rx, feet_y + h_ext.1, rz), feet_y, h_ext.1 * 2.0, radius));

        if collides {
            continue;
        }

        let prop_id = format!("{}_{}_{}", prefix, seed, placed);

        for (n_i, mut n) in p_scene.nodes.clone().into_iter().enumerate() {
            n.id = format!("{prop_id}/{n_i}");
            if let Track::Fixed(v) = n.pos {
                n.pos = Track::Fixed(v + origin);
            }
            doc.scene.nodes.push(n);
        }

        doc.colliders
            .insert(prop_id.clone(), candidate_bounds.clone());
        doc.entities.push(Entity {
            id: prop_id,
            label: format!("{} {}", prefix, kind_name),
            bounds: candidate_bounds,
            action: Action::Inspect,
        });

        placed += 1;
    }

    doc.validate()?;
    Ok((doc, placed))
}

pub fn line(
    mut doc: MapDocument,
    kind_name: &str,
    count: usize,
    p1: [f32; 2],
    p2: [f32; 2],
    prefix: Option<&str>,
) -> Result<MapDocument> {
    if count == 0 {
        return Ok(doc);
    }
    let kind = parse_prop_kind(kind_name)?;
    let p_desc = props::CATALOG.iter().find(|d| d.kind == kind).unwrap();
    let h_ext = p_desc.half_extents;
    let radius = h_ext.0.max(h_ext.2);

    let existing_colliders: Vec<Collider> = doc.colliders.values().cloned().collect();
    let p_scene = props::scene(kind);
    let prefix = prefix.unwrap_or(kind_name);

    for (mat_id, mat_val) in &p_scene.materials {
        doc.scene
            .materials
            .entry(mat_id.clone())
            .or_insert_with(|| mat_val.clone());
    }

    for i in 0..count {
        let t = if count == 1 {
            0.5
        } else {
            i as f32 / (count - 1) as f32
        };
        let rx = p1[0] + (p2[0] - p1[0]) * t;
        let rz = p1[1] + (p2[1] - p1[1]) * t;

        let feet_y = ground_support_at(rx, rz, 0.0, radius, &existing_colliders).unwrap_or(0.0);
        let origin = V(rx, feet_y, rz);
        let center = origin + V(0.0, h_ext.1, 0.0);
        let candidate_bounds = Collider {
            min: center - h_ext,
            max: center + h_ext,
        };

        let prop_id = format!("{}_line_{}", prefix, i);

        for (n_i, mut n) in p_scene.nodes.clone().into_iter().enumerate() {
            n.id = format!("{prop_id}/{n_i}");
            if let Track::Fixed(v) = n.pos {
                n.pos = Track::Fixed(v + origin);
            }
            doc.scene.nodes.push(n);
        }

        doc.colliders
            .insert(prop_id.clone(), candidate_bounds.clone());
        doc.entities.push(Entity {
            id: prop_id,
            label: format!("{} line {}", prefix, kind_name),
            bounds: candidate_bounds,
            action: Action::Inspect,
        });
    }

    doc.validate()?;
    Ok(doc)
}
