//! Simple matte props without decals or overlapping exterior trim.
mod accessories;
mod decor_library;
use super::room::Builder;
use crate::{math::V, scene::Shape};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    CerealBox,
    Chair,
    Table,
    Apple,
    FramedArt,
    FramedBotanical,
    Sculpture,
    VasePlant,
    Bowl,
    TableLamp,
    BookStack,
    CandleTrio,
    PottedCactus,
    FlowerVase,
    TallVase,
    MantelClock,
    WovenBasket,
}
pub struct PropDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PropKind,
    pub half_extents: V,
}
pub const CATALOG: [PropDefinition; 17] = [
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
    PropDefinition {
        id: "be2-framed-art",
        label: "Framed sunset print",
        kind: PropKind::FramedArt,
        half_extents: V(0.55, 0.4, 0.055),
    },
    PropDefinition {
        id: "be2-framed-botanical",
        label: "Framed botanical print",
        kind: PropKind::FramedBotanical,
        half_extents: V(0.55, 0.4, 0.055),
    },
    PropDefinition {
        id: "be2-sculpture",
        label: "Abstract terracotta sculpture",
        kind: PropKind::Sculpture,
        half_extents: V(0.25, 0.4, 0.19),
    },
    PropDefinition {
        id: "be2-vase-plant",
        label: "Leafy ceramic vase",
        kind: PropKind::VasePlant,
        half_extents: V(0.38, 0.48, 0.3),
    },
    PropDefinition {
        id: "be2-bowl",
        label: "Ceramic catchall bowl",
        kind: PropKind::Bowl,
        half_extents: V(0.24, 0.08, 0.24),
    },
    PropDefinition {
        id: "be2-table-lamp",
        label: "Table lamp",
        kind: PropKind::TableLamp,
        half_extents: V(0.23, 0.35, 0.23),
    },
    PropDefinition {
        id: "be2-book-stack",
        label: "Stacked books",
        kind: PropKind::BookStack,
        half_extents: V(0.25, 0.105, 0.18),
    },
    PropDefinition {
        id: "be2-candle-trio",
        label: "Candle trio on tray",
        kind: PropKind::CandleTrio,
        half_extents: V(0.25, 0.18, 0.18),
    },
    PropDefinition {
        id: "be2-potted-cactus",
        label: "Potted cactus",
        kind: PropKind::PottedCactus,
        half_extents: V(0.23, 0.4, 0.18),
    },
    PropDefinition {
        id: "be2-flower-vase",
        label: "Daisy vase",
        kind: PropKind::FlowerVase,
        half_extents: V(0.3, 0.4, 0.25),
    },
    PropDefinition {
        id: "be2-tall-vase",
        label: "Tall ceramic vase",
        kind: PropKind::TallVase,
        half_extents: V(0.2, 0.34, 0.2),
    },
    PropDefinition {
        id: "be2-mantel-clock",
        label: "Mantel clock",
        kind: PropKind::MantelClock,
        half_extents: V(0.29, 0.22, 0.12),
    },
    PropDefinition {
        id: "be2-woven-basket",
        label: "Woven-style basket",
        kind: PropKind::WovenBasket,
        half_extents: V(0.3, 0.19, 0.23),
    },
];
pub(super) fn palette(b: &mut Builder) {
    accessories::palette(b);
    decor_library::palette(b);
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
        PropKind::TableLamp
        | PropKind::BookStack
        | PropKind::CandleTrio
        | PropKind::PottedCactus
        | PropKind::FlowerVase
        | PropKind::TallVase
        | PropKind::MantelClock
        | PropKind::WovenBasket => decor_library::build(b, def.kind, origin),
        PropKind::FramedArt
        | PropKind::FramedBotanical
        | PropKind::Sculpture
        | PropKind::VasePlant
        | PropKind::Bowl => accessories::build(b, def.kind, origin),
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
        for def in &CATALOG[..4] {
            crate::geometry::Compiled::new(scene(def.kind), std::path::Path::new(".")).unwrap();
            let e = room.entities.iter().find(|e| e.id == def.id).unwrap();
            if matches!(def.id, "be2-chair" | "be2-table") {
                // Furniture now keeps its semantic envelope but collides by part.
                let parts: Vec<_> = room
                    .world
                    .instances
                    .iter()
                    .filter(|p| {
                        (0..3).all(|a| {
                            p.bounds.lo.axis(a) >= e.bounds.min.axis(a) - 0.001
                                && p.bounds.hi.axis(a) <= e.bounds.max.axis(a) + 0.001
                        })
                    })
                    .collect();
                assert!(parts.len() >= 5);
                for part in parts {
                    assert!(room
                        .colliders
                        .iter()
                        .any(|c| c.min == part.bounds.lo && c.max == part.bounds.hi));
                }
            } else {
                assert!(room
                    .colliders
                    .iter()
                    .any(|c| c.min == e.bounds.min && c.max == e.bounds.max));
            }
        }
    }
    #[test]
    fn accessories_compile_inside_their_inspection_and_collision_bounds() {
        for def in &CATALOG[4..] {
            let mut b = Builder {
                scene: crate::scene::Scene::default(),
                colliders: vec![],
                entities: vec![],
            };
            palette(&mut b);
            spawn(&mut b, def, V::ZERO);
            assert_eq!(b.entities[0].id, def.id);
            assert_eq!(b.entities[0].bounds.min, b.colliders[0].min);
            assert_eq!(b.entities[0].bounds.max, b.colliders[0].max);
            let compiled =
                crate::geometry::Compiled::new(b.scene, std::path::Path::new(".")).unwrap();
            for part in compiled.at(0.).instances {
                for axis in 0..3 {
                    assert!(
                        part.bounds.lo.axis(axis) >= b.colliders[0].min.axis(axis) - 0.001,
                        "{} lower axis {}",
                        def.id,
                        axis
                    );
                    assert!(
                        part.bounds.hi.axis(axis) <= b.colliders[0].max.axis(axis) + 0.001,
                        "{} upper axis {}",
                        def.id,
                        axis
                    );
                }
            }
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
            triangles <= 14000,
            "{triangles} exceeds the simple-prop triangle budget"
        );
    }
}
