//! Portable static map documents and transactional edits. No renderer dependency.
use super::{
    controller::{Collider, Controller},
    interaction::Action,
    props::{self, PropKind},
    room::{Entity, Room},
};
use crate::{
    geometry::Compiled,
    math::V,
    scene::{Material, Node, Scene, Shape, Track},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::Write,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDocument {
    pub schema_version: u32,
    pub name: String,
    pub scene: Scene,
    pub colliders: BTreeMap<String, Collider>,
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub spatial: Option<super::spatial::RoomGraph>,
}
fn finite(v: V) -> bool {
    [v.0, v.1, v.2]
        .iter()
        .all(|x| x.is_finite() && x.abs() <= 1000.)
}
fn bounds(c: &Collider) -> bool {
    finite(c.min) && finite(c.max) && c.min.0 < c.max.0 && c.min.1 < c.max.1 && c.min.2 < c.max.2
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
}
impl MapDocument {
    pub fn house() -> Result<Self> {
        Self::from_map(super::maps::MapId::House)
    }
    /// Export a built-in map, preserving geometry, colliders, semantic IDs and spatial graph.
    pub fn from_map(map: super::maps::MapId) -> Result<Self> {
        let r = super::maps::build(map)?;
        Ok(Self {
            schema_version: 1,
            name: r.name,
            scene: r.compiled.scene,
            colliders: r
                .colliders
                .into_iter()
                .enumerate()
                .map(|(i, c)| (format!("collider-{i}"), c))
                .collect(),
            entities: r.entities,
            spatial: r.spatial,
        })
    }
    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)?;
        if data.len() > 8_000_000 {
            return Err("Map exceeds 8 MB".into());
        }
        let doc: Self = serde_json::from_slice(&data)?;
        doc.validate()?;
        Ok(doc)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err("Unsupported map schema_version (expected 1)".into());
        }
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Map name must be 1..100 bytes".into());
        }
        if self.scene.nodes.len() > 5000
            || self.colliders.len() > 5000
            || self.entities.len() > 5000
        {
            return Err("Map exceeds 5000 items per collection".into());
        }
        self.scene.validate()?;
        if self.scene.audio.is_some() {
            return Err("Map documents cannot reference external audio".into());
        }
        for n in &self.scene.nodes {
            if !identifier(&n.id)
                || n.parent.is_some()
                || n.mesh.is_some()
                || !matches!(
                    n.shape,
                    Shape::Box | Shape::Sphere | Shape::Cylinder | Shape::Cone
                )
                || n.repeat.count != 1
                || n.visible.is_some()
                || serde_json::to_value(&n.motion)?
                    != serde_json::to_value(crate::scene::Motion::default())?
            {
                return Err(format!(
                    "{}: map v1 requires unparented static primitive nodes",
                    n.id
                )
                .into());
            }
            for t in [&n.pos, &n.rot, &n.scale] {
                if !matches!(t,Track::Fixed(v) if finite(*v)) {
                    return Err(format!("{}: finite fixed transforms required", n.id).into());
                }
            }
            if let Track::Fixed(v) = n.scale {
                if v.0 <= 0. || v.1 <= 0. || v.2 <= 0. {
                    return Err("Map scales must be positive".into());
                }
            }
        }
        for (id, c) in &self.colliders {
            if !identifier(id) || !bounds(c) {
                return Err(format!("Invalid collider: {id}").into());
            }
        }
        let mut ids = HashSet::new();
        for e in &self.entities {
            if !identifier(&e.id)
                || !ids.insert(&e.id)
                || e.label.len() > 100
                || !bounds(&e.bounds)
                || e.action != Action::Inspect
            {
                return Err(
                    format!("Invalid/duplicate entity or unsupported action: {}", e.id).into(),
                );
            }
        }
        if self
            .colliders
            .values()
            .any(|c| c.blocks(Controller::default().position))
        {
            return Err("Default player spawn is blocked".into());
        }
        Ok(())
    }
    pub fn build(&self) -> Result<Room> {
        self.validate()?;
        let compiled = Compiled::new(self.scene.clone(), Path::new("."))?;
        let world = compiled.at(0.);
        Ok(Room {
            name: self.name.clone(),
            simple_geometry: true,
            compiled,
            world,
            dynamic_world: crate::geometry::World::new(vec![]),
            colliders: self.colliders.values().cloned().collect(),
            entities: self.entities.clone(),
            spatial: self.spatial.clone(),
        }
        .with_furniture_colliders())
    }
    pub fn apply(&self, operations: &[Edit]) -> Result<Self> {
        let mut next = self.clone();
        if operations.len() > 1000 {
            return Err("At most 1000 operations per patch".into());
        }
        for operation in operations {
            next.edit(operation)?;
        }
        next.validate()?;
        // Compile before returning so callers cannot save an invalid geometry expansion.
        next.build()?;
        Ok(next)
    }
    fn edit(&mut self, op: &Edit) -> Result<()> {
        match op {
            Edit::AddBox {
                id,
                label,
                center,
                half_extents,
                color,
            } => {
                self.free_id(id)?;
                if !finite(*color)
                    || [color.0, color.1, color.2]
                        .iter()
                        .any(|v| !(0.0..=1.0).contains(v))
                {
                    return Err("Color must be linear RGB in 0..1".into());
                }
                let mat = format!("edit-{id}");
                self.scene.materials.insert(
                    mat.clone(),
                    Material {
                        color: *color,
                        roughness: 1.,
                        ..Default::default()
                    },
                );
                self.scene.nodes.push(Node {
                    id: id.clone(),
                    shape: Shape::Box,
                    material: mat,
                    pos: Track::Fixed(*center),
                    scale: Track::Fixed(*half_extents),
                    ..Default::default()
                });
                self.attach(
                    id,
                    label,
                    Collider {
                        min: *center - *half_extents,
                        max: *center + *half_extents,
                    },
                );
            }
            Edit::AddProp {
                id,
                label,
                kind,
                origin,
            } => {
                self.free_id(id)?;
                let k = match kind.as_str() {
                    "apple" => PropKind::Apple,
                    "table-lamp" => PropKind::TableLamp,
                    "book-stack" => PropKind::BookStack,
                    "candle-trio" => PropKind::CandleTrio,
                    "potted-cactus" => PropKind::PottedCactus,
                    "flower-vase" => PropKind::FlowerVase,
                    "tall-vase" => PropKind::TallVase,
                    "mantel-clock" => PropKind::MantelClock,
                    "woven-basket" => PropKind::WovenBasket,

                    "framed-art" => PropKind::FramedArt,
                    "framed-botanical" => PropKind::FramedBotanical,
                    "sculpture" => PropKind::Sculpture,
                    "vase-plant" => PropKind::VasePlant,
                    "bowl" => PropKind::Bowl,

                    "cereal" => PropKind::CerealBox,
                    "chair" => PropKind::Chair,
                    "table" => PropKind::Table,
                    _ => {
                        return Err("Unknown prop kind; use catalog to list supported kinds".into())
                    }
                };
                let scene = props::scene(k);
                for (key, value) in scene.materials {
                    self.scene.materials.entry(key).or_insert(value);
                }
                for (i, mut n) in scene.nodes.into_iter().enumerate() {
                    n.id = format!("{id}/{i}");
                    if let Track::Fixed(v) = n.pos {
                        n.pos = Track::Fixed(v + *origin);
                    }
                    self.scene.nodes.push(n);
                }
                let h = props::CATALOG
                    .iter()
                    .find(|d| d.kind == k)
                    .unwrap()
                    .half_extents;
                let center = *origin + V(0., h.1, 0.);
                self.attach(
                    id,
                    label,
                    Collider {
                        min: center - h,
                        max: center + h,
                    },
                );
            }
            Edit::Translate {
                nodes,
                colliders,
                entities,
                delta,
            } => {
                self.selection(nodes, colliders, entities)?;
                if !finite(*delta) {
                    return Err("Invalid translation".into());
                }
                for n in &mut self.scene.nodes {
                    if nodes.contains(&n.id) {
                        if let Track::Fixed(v) = n.pos {
                            n.pos = Track::Fixed(v + *delta);
                        }
                    }
                }
                for (id, c) in &mut self.colliders {
                    if colliders.contains(id) {
                        c.min = c.min + *delta;
                        c.max = c.max + *delta;
                    }
                }
                for e in &mut self.entities {
                    if entities.contains(&e.id) {
                        e.bounds.min = e.bounds.min + *delta;
                        e.bounds.max = e.bounds.max + *delta;
                    }
                }
            }
            Edit::Remove {
                nodes,
                colliders,
                entities,
            } => {
                self.selection(nodes, colliders, entities)?;
                self.scene.nodes.retain(|n| !nodes.contains(&n.id));
                self.colliders.retain(|id, _| !colliders.contains(id));
                self.entities.retain(|e| !entities.contains(&e.id));
            }
        }
        Ok(())
    }
    fn free_id(&self, id: &str) -> Result<()> {
        if !identifier(id)
            || self
                .scene
                .nodes
                .iter()
                .any(|n| n.id == id || n.id.starts_with(&format!("{id}/")))
            || self.entities.iter().any(|e| e.id == id)
            || self.colliders.contains_key(id)
        {
            return Err(format!("Invalid or occupied object ID: {id}").into());
        }
        Ok(())
    }
    fn attach(&mut self, id: &str, label: &str, b: Collider) {
        self.colliders.insert(id.into(), b.clone());
        self.entities.push(Entity {
            id: id.into(),
            label: label.into(),
            bounds: b,
            action: Action::Inspect,
        });
    }
    fn selection(&self, n: &[String], c: &[String], e: &[String]) -> Result<()> {
        if n.is_empty() && c.is_empty() && e.is_empty() {
            return Err("Empty selection".into());
        }
        for id in n {
            if !self.scene.nodes.iter().any(|v| v.id == *id) {
                return Err(format!("Unknown node: {id}").into());
            }
        }
        for id in c {
            if !self.colliders.contains_key(id) {
                return Err(format!("Unknown collider: {id}").into());
            }
        }
        for id in e {
            if !self.entities.iter().any(|v| v.id == *id) {
                return Err(format!("Unknown entity: {id}").into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    AddBox {
        id: String,
        label: String,
        center: V,
        half_extents: V,
        color: V,
    },
    AddProp {
        id: String,
        label: String,
        kind: String,
        origin: V,
    },
    Translate {
        nodes: Vec<String>,
        colliders: Vec<String>,
        entities: Vec<String>,
        delta: V,
    },
    Remove {
        nodes: Vec<String>,
        colliders: Vec<String>,
        entities: Vec<String>,
    },
}
/// Exclusive creation never replaces an existing file, including in a creation race.
pub fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(e) = f.write_all(data).and_then(|()| f.sync_all()) {
        drop(f);
        let _ = std::fs::remove_file(path);
        return Err(e.into());
    }
    Ok(())
}
