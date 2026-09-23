//! Procedural prop catalog. Origins are floor-level; bounds are half extents.
use super::room::Builder;
use crate::{math::V, scene::Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Crate,
    Barrel,
    Stool,
    Toolbox,
}
pub struct PropDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PropKind,
    pub half_extents: V,
}
pub const CATALOG: [PropDefinition; 4] = [
    PropDefinition {
        id: "be2-crate",
        label: "Shipping crate",
        kind: PropKind::Crate,
        half_extents: V(0.40, 0.40, 0.40),
    },
    PropDefinition {
        id: "be2-barrel",
        label: "Blue steel barrel",
        kind: PropKind::Barrel,
        half_extents: V(0.34, 0.52, 0.34),
    },
    PropDefinition {
        id: "be2-stool",
        label: "Workshop stool",
        kind: PropKind::Stool,
        half_extents: V(0.30, 0.36, 0.30),
    },
    PropDefinition {
        id: "be2-toolbox",
        label: "Mechanic toolbox",
        kind: PropKind::Toolbox,
        half_extents: V(0.34, 0.23, 0.20),
    },
];

/// Standalone reusable scene at the origin, also exportable as Vesper JSON.
pub fn scene(kind: PropKind) -> crate::scene::Scene {
    let mut b = Builder {
        scene: crate::scene::Scene::default(),
        colliders: vec![],
        entities: vec![],
    };
    for (id, color, metal) in [
        ("oak", V(0.36, 0.20, 0.085), 0.),
        ("trim", V(0.023, 0.032, 0.043), 0.6),
        ("paper", V(0.86, 0.79, 0.61), 0.),
        ("navy", V(0.018, 0.045, 0.09), 0.),
        ("blue", V(0.015, 0.18, 0.56), 0.3),
        ("brass", V(0.65, 0.37, 0.10), 0.75),
    ] {
        b.material(id, color, metal, 0.);
    }
    let def = CATALOG.iter().find(|d| d.kind == kind).unwrap();
    spawn(&mut b, def, V::ZERO);
    for (index, node) in b.scene.nodes.iter_mut().enumerate() {
        node.id = format!("{}-{index}", def.id);
    }
    b.scene.camera.pos = crate::scene::Track::Fixed(V(2., 1.6, 2.5));
    b.scene.camera.target = crate::scene::Track::Fixed(V(0., def.half_extents.1, 0.));
    b.scene
}

pub(super) fn populate(b: &mut Builder) {
    for (def, origin) in CATALOG.iter().zip([
        V(2.7, 0., 3.8),
        V(3.7, 0., 3.8),
        V(-2.8, 0., 2.0),
        V(1.7, 0., 3.8),
    ]) {
        spawn(b, def, origin);
    }
}
fn spawn(b: &mut Builder, def: &PropDefinition, origin: V) {
    let center = origin + V(0., def.half_extents.1, 0.);
    match def.kind {
        PropKind::Crate => {
            b.cube("oak", center, V(0.38, 0.38, 0.38));
            for x in [-0.34, 0.34] {
                for z in [-0.39, 0.39] {
                    b.cube("trim", center + V(x, 0., z), V(0.045, 0.40, 0.01));
                }
            }
            for y in [-0.30, 0.30] {
                b.cube("oak", center + V(0., y, -0.39), V(0.40, 0.045, 0.01));
            }
            b.cube("paper", center + V(0., 0.06, -0.401), V(0.12, 0.08, 0.002));
            for y in [0.03, 0.07, 0.11] {
                b.cube("navy", center + V(0., y, -0.404), V(0.08, 0.008, 0.001));
            }
        }
        PropKind::Barrel => {
            b.add(
                Shape::Cylinder,
                "blue",
                center,
                V(0.32, 0.50, 0.32),
                V::ZERO,
            );
            for y in [-0.48, -0.30, 0.30, 0.48] {
                b.add(
                    Shape::Cylinder,
                    "trim",
                    center + V(0., y, 0.),
                    V(0.34, 0.025, 0.34),
                    V::ZERO,
                );
            }
            b.add(
                Shape::Cylinder,
                "brass",
                center + V(0.14, 0.515, 0.),
                V(0.045, 0.005, 0.045),
                V::ZERO,
            );
        }
        PropKind::Stool => {
            b.add(
                Shape::Cylinder,
                "oak",
                origin + V(0., 0.68, 0.),
                V(0.30, 0.04, 0.30),
                V::ZERO,
            );
            for x in [-0.19, 0.19] {
                for z in [-0.19, 0.19] {
                    b.cube("trim", origin + V(x, 0.32, z), V(0.028, 0.32, 0.028));
                }
            }
            for x in [-0.19, 0.19] {
                b.cube("trim", origin + V(x, 0.22, 0.), V(0.025, 0.025, 0.19));
            }
        }
        PropKind::Toolbox => {
            b.cube("blue", origin + V(0., 0.16, 0.), V(0.34, 0.16, 0.20));
            b.cube("trim", origin + V(0., 0.32, 0.), V(0.34, 0.025, 0.20));
            for x in [-0.12, 0.12] {
                b.cube("brass", origin + V(x, 0.36, 0.), V(0.025, 0.08, 0.025));
            }
            b.cube("navy", origin + V(0., 0.435, 0.), V(0.145, 0.025, 0.03));
            for x in [-0.20, 0.20] {
                b.cube("brass", origin + V(x, 0.25, -0.202), V(0.035, 0.055, 0.007));
            }
        }
    }
    b.obstacle(center, def.half_extents);
    b.entity(def.id, def.label, center, def.half_extents);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_props_have_stable_entities_and_colliders() {
        let room = super::super::room::build().unwrap();
        for def in &CATALOG {
            crate::geometry::Compiled::new(scene(def.kind), std::path::Path::new(".")).unwrap();
            let e = room.entities.iter().find(|e| e.id == def.id).unwrap();
            assert!(room
                .colliders
                .iter()
                .any(|c| c.min == e.bounds.min && c.max == e.bounds.max));
        }
    }
}
