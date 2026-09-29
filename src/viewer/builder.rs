//! Declarative authoring over the existing validated map contract, without borrowed handles.
use super::authoring::{Edit, MapDocument, MapSpawn};
use super::simulation::HeadlessWorld;
use crate::{math::V, scene::Scene, Result};
use std::collections::BTreeMap;

/// Owns a batch of edits. Validation/compilation happens once, at [`Self::build`].
/// IDs are explicit, owned strings; duplicate IDs fail instead of replacing objects.
/// Boxes have matching visual, collider and semantic components. Catalog props use
/// the existing loose-body extraction rules (large furniture/wall art stay fixed).
pub struct SceneBuilder {
    name: String,
    edits: Vec<Edit>,
    default_spawn: Option<MapSpawn>,
    spawnless: bool,
}

impl SceneBuilder {
    /// Start an empty map. Set [`Self::spawn`] (or [`Self::without_spawn`]) and include geometry before building.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            edits: Vec::new(),
            default_spawn: None,
            spawnless: false,
        }
    }

    /// Set the required standalone player spawn using feet coordinates and yaw radians.
    pub fn spawn(mut self, feet: V, yaw: f32) -> Self {
        self.default_spawn = Some(MapSpawn { feet, yaw });
        self
    }

    /// Build a props-only world with no player spawn: a physics authority a game steps from its own rules
    /// (a ball on a course, a target range). `HeadlessWorld::join` then returns false, and the document cannot
    /// be used for standalone play or `change_map`, which need a spawn.
    pub fn without_spawn(mut self) -> Self {
        self.spawnless = true;
        self
    }

    /// Add a static box: metres, Y-up, center and positive half extents, linear RGB.
    pub fn box_body(mut self, id: impl Into<String>, center: V, half_extents: V, color: V) -> Self {
        let id = id.into();
        self.edits.push(Edit::AddBox {
            label: id.clone(),
            id,
            center,
            half_extents,
            color,
            structural: false,
        });
        self
    }

    /// Add visual static collision without creating a semantic gameplay entity.
    pub fn structural_box(
        mut self,
        id: impl Into<String>,
        center: V,
        half_extents: V,
        color: V,
    ) -> Self {
        let id = id.into();
        self.edits.push(Edit::AddBox {
            label: id.clone(),
            id,
            center,
            half_extents,
            color,
            structural: true,
        });
        self
    }

    /// Add catalog geometry, collision and semantic ID at its bottom origin.
    /// Kinds are listed by `be2-tools catalog`; unknown kinds fail during build.
    pub fn prop(mut self, id: impl Into<String>, kind: impl Into<String>, origin: V) -> Self {
        let id = id.into();
        self.edits.push(Edit::AddProp {
            label: id.clone(),
            id,
            kind: kind.into(),
            origin,
        });
        self
    }

    /// Validate the complete transaction, including spawn clearance, and compile it.
    /// Returns the ordinary MapDocument used by both `be2 --map` and the headless host.
    pub fn build(self) -> Result<MapDocument> {
        if self.default_spawn.is_none() && !self.spawnless {
            return Err("SceneBuilder requires spawn(feet, yaw) or without_spawn()".into());
        }
        MapDocument {
            schema_version: 1,
            name: self.name,
            scene: Scene::default(),
            colliders: BTreeMap::new(),
            entities: Vec::new(),
            default_spawn: self.default_spawn,
            spatial: None,
            checks: None,
        }
        .apply(&self.edits)
    }

    /// Construct a simulation, reporting physics initialization failures to the caller.
    pub fn world(self) -> Result<HeadlessWorld> {
        HeadlessWorld::try_with_room(self.build()?.build()?)
    }
}
