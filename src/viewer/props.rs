//! Simple matte props without decals or overlapping exterior trim.
use super::room::Builder;
use crate::{math::V, scene::Shape};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    CerealBox,
    Chair,
    Table,
    Apple,
}
pub struct PropDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PropKind,
    pub half_extents: V,
}
pub const CATALOG: [PropDefinition; 4] = [
    PropDefinition {
        id: "be2-cereal-box",
        label: "Cereal box",
        kind: PropKind::CerealBox,
        half_extents: V(0.14, 0.22, 0.07),
    },
    PropDefinition {
        id: "be2-chair",
        label: "Chair",
        kind: PropKind::Chair,
        half_extents: V(0.26, 0.47, 0.26),
    },
    PropDefinition {
        id: "be2-table",
        label: "Table",
        kind: PropKind::Table,
        half_extents: V(0.80, 0.40, 0.50),
    },
    PropDefinition {
        id: "be2-apple",
        label: "Apple",
        kind: PropKind::Apple,
        half_extents: V(0.10, 0.12, 0.10),
    },
];
pub(super) fn palette(b: &mut Builder) {
    for (id, color) in [
        ("prop-yellow", V(0.95, 0.62, 0.08)),
        ("prop-blue", V(0.04, 0.23, 0.62)),
        ("prop-wood", V(0.48, 0.27, 0.12)),
        ("prop-red", V(0.75, 0.025, 0.02)),
        ("prop-stem", V(0.15, 0.075, 0.025)),
    ] {
        b.material(id, color, 0., 0.);
        b.scene.materials.get_mut(id).unwrap().roughness = 1.;
    }
}
pub fn scene(kind: PropKind) -> crate::scene::Scene {
    let mut b = Builder {
        scene: crate::scene::Scene::default(),
        colliders: vec![],
        entities: vec![],
    };
    palette(&mut b);
    let def = CATALOG.iter().find(|d| d.kind == kind).unwrap();
    spawn(&mut b, def, V::ZERO);
    for (index, node) in b.scene.nodes.iter_mut().enumerate() {
        node.id = format!("{}-{index}", def.id);
    }
    let scale = def.half_extents.length() * 2.5;
    b.scene.camera.pos = crate::scene::Track::Fixed(V(scale, scale * 0.8, scale));
    b.scene.camera.target = crate::scene::Track::Fixed(V(0., def.half_extents.1, 0.));
    b.scene
}
pub(super) fn populate(b: &mut Builder) {
    palette(b);
    for (def, origin) in CATALOG.iter().zip([
        V(2.32, 0.80, 3.8),
        V(1.25, 0., 3.8),
        V(2.7, 0., 3.8),
        V(3.08, 0.80, 3.8),
    ]) {
        spawn(b, def, origin);
    }
}
/// Reuse geometry under a map-specific instance ID.
pub(super) fn place(
    b: &mut Builder,
    kind: PropKind,
    id: &'static str,
    label: &'static str,
    origin: V,
) {
    let source = CATALOG.iter().find(|p| p.kind == kind).unwrap();
    let def = PropDefinition {
        id,
        label,
        kind,
        half_extents: source.half_extents,
    };
    spawn(b, &def, origin);
}
fn spawn(b: &mut Builder, def: &PropDefinition, origin: V) {
    let center = origin + V(0., def.half_extents.1, 0.);
    match def.kind {
        PropKind::CerealBox => {
            // Abutting carton sections, not a label laid over an existing face.
            for (mat, y, h) in [
                ("prop-yellow", 0.055, 0.055),
                ("prop-blue", 0.22, 0.11),
                ("prop-yellow", 0.385, 0.055),
            ] {
                b.cube(mat, origin + V(0., y, 0.), V(0.14, h, 0.07));
            }
        }
        PropKind::Chair => {
            b.cube("prop-blue", origin + V(0., 0.44, 0.), V(0.26, 0.04, 0.26));
            for x in [-0.21, 0.21] {
                for z in [-0.21, 0.21] {
                    b.cube("prop-wood", origin + V(x, 0.20, z), V(0.04, 0.20, 0.04));
                }
            }
            b.cube("prop-blue", origin + V(0., 0.71, 0.22), V(0.26, 0.23, 0.04));
        }
        PropKind::Table => {
            b.cube("prop-wood", origin + V(0., 0.75, 0.), V(0.80, 0.05, 0.50));
            for x in [-0.68, 0.68] {
                for z in [-0.38, 0.38] {
                    b.cube("prop-wood", origin + V(x, 0.35, z), V(0.06, 0.35, 0.06));
                }
            }
        }
        PropKind::Apple => {
            b.add(
                Shape::Sphere,
                "prop-red",
                origin + V(0., 0.10, 0.),
                V(0.10, 0.10, 0.10),
                V::ZERO,
            );
            b.cube(
                "prop-stem",
                origin + V(0., 0.217, 0.),
                V(0.012, 0.023, 0.012),
            );
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
    #[cfg(feature = "client")]
    #[test]
    fn simple_props_have_small_meshes_and_matte_tags() {
        use super::super::{controller::Collider, mesh};
        let mut triangles = 0;
        for def in &CATALOG {
            let compiled =
                crate::geometry::Compiled::new(scene(def.kind), std::path::Path::new(".")).unwrap();
            let world = compiled.at(0.);
            let meshes = mesh::bake_tagged(
                &world,
                &[(
                    Collider {
                        min: V(-2., -2., -2.),
                        max: V(2., 2., 2.),
                    },
                    3.,
                )],
            );
            for m in meshes {
                triangles += m.indices.len() / 3;
                assert!(m.vertices.iter().all(|v| v.uv.x == 3. && v.normal.w == 0.));
                assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
            }
        }
        assert!(
            triangles <= 360,
            "{triangles} exceeds the simple-prop triangle budget"
        );
    }
}
