//! Declarative authoring over the existing validated map contract, without borrowed handles.
use super::authoring::{Edit, MapDocument};
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
}

impl SceneBuilder {
    /// Start an empty map. Include at least one box or catalog prop before building.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            edits: Vec::new(),
        }
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
        MapDocument {
            schema_version: 1,
            name: self.name,
            scene: Scene::default(),
            colliders: BTreeMap::new(),
            entities: Vec::new(),
            spatial: None,
        }
        .apply(&self.edits)
    }

    /// Construct a simulation, reporting physics initialization failures to the caller.
    pub fn world(self) -> Result<HeadlessWorld> {
        HeadlessWorld::try_with_room(self.build()?.build()?)
    }
}
