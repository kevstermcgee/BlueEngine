use super::controller::Collider;
use super::interaction::Action;
use crate::{
    geometry::{Compiled, World},
    math::V,
    scene::{Material, Node, Scene, Shape, Track},
};
use std::path::Path;

/// Stable IDs and bounds are the attachment points for future interaction components.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub label: String,
    pub bounds: Collider,
    pub action: Action,
}
pub struct Room {
    pub name: String,
    pub simple_geometry: bool,
    pub compiled: Compiled,
    pub world: World,
    /// Moving geometry, rebuilt independently of the static map BVH.
    pub dynamic_world: World,
    pub colliders: Vec<Collider>,
    pub entities: Vec<Entity>,
    /// Optional embedded spatial room graph for portals and interest management.
    pub spatial: Option<super::spatial::RoomGraph>,
}
impl Room {
    /// Replace legacy furniture envelopes with their visible component bounds.
    /// Semantic bounds remain intact for inspection and rigid-body ownership.
    pub(crate) fn with_furniture_colliders(mut self) -> Self {
        for entity in &self.entities {
            let label = entity.label.to_ascii_lowercase();
            let furniture = label.split([' ', '-']).next_back().is_some_and(|word| {
                matches!(word, "table" | "desk" | "chair" | "bench" | "workbench")
            });
            if !furniture {
                continue;
            }
            let envelope = &entity.bounds;
            let matches = |c: &Collider| {
                (c.min - envelope.min).length() < 0.005 && (c.max - envelope.max).length() < 0.005
            };
            if !self.colliders.iter().any(matches) {
                continue;
            }
            let parts: Vec<_> = self
                .world
                .instances
                .iter()
                .filter(|part| {
                    (0..3).all(|axis| {
                        part.bounds.lo.axis(axis) >= envelope.min.axis(axis) - 0.003
                            && part.bounds.hi.axis(axis) <= envelope.max.axis(axis) + 0.003
                    })
                })
                .map(|part| Collider {
                    min: part.bounds.lo,
                    max: part.bounds.hi,
                })
                .collect();
            // A single solid plinth is not a passage; unmatched/incomplete data
            // keeps the authored proxy. Never infer a hole by lowering its top.
            if parts.len() > 1 {
                self.colliders.retain(|c| !matches(c));
                self.colliders.extend(parts);
            }
        }
        self
    }

    /// Closest static or moving surface; used by aim, tools and camera occlusion.
    pub fn hit(&self, ray: crate::math::Ray, distance: f32) -> Option<crate::geometry::Hit> {
        let fixed = self.world.hit(ray, distance, false);
        self.dynamic_world
            .hit(ray, fixed.map_or(distance, |h| h.t), false)
            .or(fixed)
    }

    /// GPU tags change only the selected surfaces; static geometry and shadows stay valid.
    pub fn render_tags(&self) -> Vec<(Collider, f32)> {
        if self.simple_geometry {
            return vec![(
                Collider {
                    min: V(-10000., -10000., -10000.),
                    max: V(10000., 10000., 10000.),
                },
                3.,
            )];
        }
        let mut tags = vec![
            (
                Collider {
                    min: V(-4.85, 0.98, -2.17),
                    max: V(-4.79, 1.70, -1.03),
                },
                1.,
            ),
            (
                self.entities
                    .iter()
                    .find(|e| e.id == "vesper-crystal")
                    .unwrap()
                    .bounds
                    .clone(),
                2.,
            ),
        ];
        tags.extend(
            self.entities
                .iter()
                .filter(|e| super::props::CATALOG.iter().any(|p| p.id == e.id))
                .map(|e| (e.bounds.clone(), 3.)),
        );
        tags
    }
    pub fn focus(&self, ray: crate::math::Ray) -> Option<&Entity> {
        let hit = self.hit(ray, 4.5)?;
        self.entities.iter().find(|e| e.bounds.contains(hit.p))
    }
}
pub(super) struct Builder {
    pub(super) scene: Scene,
    pub(super) colliders: Vec<Collider>,
    pub(super) entities: Vec<Entity>,
}
impl Builder {
    pub(super) fn material(&mut self, id: &str, rgb: V, metallic: f32, emission: f32) {
        self.scene.materials.insert(
            id.into(),
            Material {
                color: rgb,
                metallic,
                emission,
                roughness: 0.4,
                checker: None,
            },
        );
    }
    pub(super) fn add(&mut self, shape: Shape, mat: &str, pos: V, scale: V, rot: V) {
        self.scene.nodes.push(Node {
            id: format!("room-{}", self.scene.nodes.len()),
            shape,
            material: mat.into(),
            pos: Track::Fixed(pos),
            scale: Track::Fixed(scale),
            rot: Track::Fixed(rot),
            ..Default::default()
        });
    }
    pub(super) fn cube(&mut self, mat: &str, p: V, s: V) {
        self.add(Shape::Box, mat, p, s, V::ZERO);
    }
    pub(super) fn obstacle(&mut self, p: V, s: V) {
        self.colliders.push(Collider {
            min: p - s,
            max: p + s,
        });
    }
    pub(super) fn entity(&mut self, id: &'static str, label: &'static str, p: V, s: V) {
        self.entities.push(Entity {
            id: id.into(),
            label: label.into(),
            bounds: Collider {
                min: p - s,
                max: p + s,
            },
            action: match id {
                "monitor" => Action::ToggleMonitor,
                "vesper-crystal" => Action::CycleCrystal,
                _ => Action::Inspect,
            },
        });
    }
}
pub fn build() -> crate::Result<Room> {
    let mut b = Builder {
        scene: Scene::default(),
        colliders: vec![],
        entities: vec![],
    };
    b.scene.nodes.clear();
    for (name, rgb, metal, emission) in [
        ("plaster", V(0.55, 0.58, 0.55), 0., 0.),
        ("ceiling", V(0.62, 0.66, 0.67), 0., 0.),
        ("navy", V(0.018, 0.045, 0.09), 0., 0.),
        ("blue", V(0.015, 0.18, 0.56), 0.3, 0.),
        ("oak", V(0.36, 0.20, 0.085), 0., 0.),
        ("wood-light", V(0.48, 0.31, 0.15), 0., 0.),
        ("wood-dark", V(0.29, 0.15, 0.055), 0., 0.),
        ("trim", V(0.023, 0.032, 0.043), 0.6, 0.),
        ("linen", V(0.54, 0.58, 0.53), 0., 0.),
        ("rug", V(0.07, 0.16, 0.22), 0., 0.),
        ("brass", V(0.65, 0.37, 0.10), 0.75, 0.),
        ("ceramic", V(0.8, 0.75, 0.6), 0.1, 0.),
        ("green", V(0.055, 0.20, 0.10), 0., 0.),
        ("light", V(0.8, 0.87, 1.), 0., 1.4),
        ("warm-light", V(1., 0.66, 0.28), 0., 1.2),
        ("sky", V(0.25, 0.51, 0.71), 0., 1.0),
        ("skyline", V(0.17, 0.30, 0.40), 0., 0.4),
        ("paper", V(0.86, 0.79, 0.61), 0., 0.),
    ] {
        b.material(name, rgb, metal, emission);
    }

    // 12 x 12 metre room. Seams and material variation give the floor scale cues.
    for x in 0..24 {
        for z in 0..6 {
            let mat = match (x * 7 + z * 3) % 5 {
                0 => "wood-light",
                1 => "wood-dark",
                _ => "oak",
            };
            b.cube(
                mat,
                V(-5.75 + x as f32 * 0.5, -0.055, -5. + z as f32 * 2.),
                V(0.248, 0.05, 0.998),
            );
        }
    }
    b.cube("ceiling", V(0., 3.65, 0.), V(6., 0.10, 6.));
    b.cube("navy", V(0., 1.8, -6.), V(6., 1.8, 0.12));
    b.cube("plaster", V(0., 1.8, 6.), V(6., 1.8, 0.12));
    b.cube("plaster", V(6., 1.8, 0.), V(0.12, 1.8, 6.));
    // West window recess: solid low wall, lintel, and piers.
    b.cube("plaster", V(-6., 0.4, 0.), V(0.12, 0.4, 6.));
    b.cube("plaster", V(-6., 3.35, 0.), V(0.12, 0.25, 6.));
    for z in [-5.6, 5.6] {
        b.cube("plaster", V(-6., 1.9, z), V(0.12, 1.2, 0.4));
    }
    b.cube("sky", V(-6.18, 1.95, 0.), V(0.05, 1.15, 5.2));
    for i in 0..17 {
        let h = 0.3 + ((i * 13) % 7) as f32 * 0.09;
        b.cube(
            "skyline",
            V(-6.10, 0.85 + h, -4.9 + i as f32 * 0.60),
            V(0.02, h, 0.23),
        );
    }
    for z in [-5.15, -2.6, 0., 2.6, 5.15] {
        b.cube("trim", V(-5.93, 1.95, z), V(0.10, 1.20, 0.045));
    }
    b.cube("oak", V(-5.86, 0.80, 0.), V(0.23, 0.045, 5.3));
    b.obstacle(V(0., 3.65, 0.), V(6., 0.10, 6.));
    for x in [-6., 6.] {
        b.obstacle(V(x, 1.8, 0.), V(0.14, 1.8, 6.2));
    }
    for z in [-6., 6.] {
        b.obstacle(V(0., 1.8, z), V(6.2, 1.8, 0.14));
        b.cube("trim", V(0., 0.08, z.signum() * 5.85), V(5.9, 0.08, 0.035));
    }
    for x in [-2.8, 2.8] {
        b.cube("trim", V(x, 3.48, 0.), V(0.06, 0.04, 4.5));
        for z in [-3., 0., 3.] {
            b.cube("light", V(x, 3.42, z), V(0.18, 0.025, 0.65));
        }
    }
    // Blue artwork on the back wall, composed in the original scene language.
    b.cube("trim", V(-0.5, 2.05, -5.80), V(2.15, 1.05, 0.06));
    b.cube("paper", V(-0.5, 2.05, -5.72), V(2.06, 0.96, 0.025));
    b.add(
        Shape::Sphere,
        "blue",
        V(-1.10, 2.15, -5.65),
        V(0.68, 0.68, 0.025),
        V::ZERO,
    );
    b.add(
        Shape::Torus,
        "brass",
        V(0.65, 2.1, -5.59),
        V(0.66, 0.08, 0.66),
        V(90., 0., 0.),
    );
    b.cube("navy", V(-0.1, 1.43, -5.60), V(1.45, 0.025, 0.018));
    b.entity(
        "composition-01",
        "Composition / 01",
        V(-0.5, 2.05, -5.65),
        V(2.2, 1.1, 0.25),
    );
    // Lounge, deliberately offset so there is a clear circulation route.
    b.cube("rug", V(2.25, 0.006, 0.25), V(2.1, 0.008, 2.35));
    for i in 0..24 {
        b.cube(
            "linen",
            V(0.24 + i as f32 * 0.175, 0.016, 2.52),
            V(0.018, 0.003, 0.065),
        );
    }
    b.cube("trim", V(4.65, 0.21, 0.15), V(0.65, 0.15, 1.8));
    b.cube("linen", V(4.65, 0.54, 0.15), V(0.65, 0.18, 1.8));
    b.cube("linen", V(5.19, 0.96, 0.15), V(0.17, 0.45, 1.83));
    for z in [-1.65, 1.95] {
        b.cube("linen", V(4.65, 0.78, z), V(0.67, 0.40, 0.15));
    }
    for z in [-0.92, 0.15, 1.22] {
        b.cube("linen", V(4.57, 0.75, z), V(0.51, 0.05, 0.51));
    }
    for z in [-1.05, 1.25] {
        b.add(
            Shape::Box,
            "blue",
            V(4.86, 1.04, z),
            V(0.17, 0.28, 0.33),
            V(0., 0., -12.),
        );
    }
    b.obstacle(V(4.65, 0.70, 0.15), V(0.85, 0.70, 2.));
    b.entity(
        "lounge",
        "Linen lounge",
        V(4.65, 0.75, 0.15),
        V(0.9, 0.8, 2.05),
    );
    b.cube("oak", V(2.05, 0.57, 0.25), V(0.75, 0.06, 1.10));
    for x in [1.48, 2.62] {
        for z in [-0.6, 1.10] {
            b.cube("trim", V(x, 0.28, z), V(0.04, 0.28, 0.04));
        }
    }
    b.obstacle(V(2.05, 0.32, 0.25), V(0.78, 0.32, 1.13));
    b.cube("paper", V(2.12, 0.66, 0.75), V(0.32, 0.035, 0.22));
    b.cube("blue", V(2.12, 0.705, 0.75), V(0.32, 0.009, 0.22));
    b.add(
        Shape::Cylinder,
        "ceramic",
        V(1.85, 0.78, -0.2),
        V(0.10, 0.13, 0.10),
        V::ZERO,
    );
    b.entity(
        "notebook",
        "Studio notebook",
        V(2.12, 0.69, 0.75),
        V(0.33, 0.065, 0.23),
    );
    b.entity(
        "table",
        "Reading table",
        V(2.05, 0.5, 0.25),
        V(0.8, 0.5, 1.15),
    );
    // Workbench along the windows.
    b.cube("oak", V(-4.62, 0.83, -1.25), V(0.67, 0.06, 1.65));
    for z in [-2.7, 0.2] {
        b.cube("trim", V(-4.62, 0.4, z), V(0.49, 0.4, 0.045));
    }
    b.obstacle(V(-4.62, 0.45, -1.25), V(0.72, 0.45, 1.7));
    b.cube("trim", V(-4.88, 1.34, -1.6), V(0.045, 0.40, 0.62));
    b.cube("blue", V(-4.825, 1.34, -1.6), V(0.015, 0.35, 0.56));
    b.cube("light", V(-4.805, 1.35, -1.65), V(0.006, 0.017, 0.33));
    b.cube("trim", V(-4.8, 1.02, -1.6), V(0.1, 0.18, 0.045));
    b.cube("trim", V(-4.22, 0.91, -1.6), V(0.19, 0.018, 0.43));
    b.entity(
        "monitor",
        "Design terminal",
        V(-4.84, 1.34, -1.6),
        V(0.12, 0.42, 0.64),
    );
    b.entity(
        "workbench",
        "Design workbench",
        V(-4.62, 1., -1.25),
        V(0.75, 0.9, 1.75),
    );
    // Plinth and Vesper3D's original crystal model.
    b.cube("plaster", V(-1.65, 0.49, -2.80), V(0.57, 0.49, 0.57));
    b.cube("trim", V(-1.65, 1.0, -2.80), V(0.59, 0.025, 0.59));
    b.add(
        Shape::Crystal,
        "blue",
        V(-1.65, 1.52, -2.80),
        V(0.39, 0.58, 0.39),
        V(0., 24., 0.),
    );
    b.obstacle(V(-1.65, 0.7, -2.80), V(0.6, 0.7, 0.6));
    b.entity(
        "vesper-crystal",
        "Vesper / crystal study",
        V(-1.65, 1.70, -2.80),
        V(0.5, 0.65, 0.5),
    );
    // Built-in display shelving and familiar Vesper robot.
    for x in [3.20, 5.5] {
        b.cube("oak", V(x, 1.5, -5.28), V(0.055, 1.5, 0.43));
    }
    for y in [0.25, 1.15, 2.15, 3.0] {
        b.cube("oak", V(4.35, y, -5.28), V(1.2, 0.045, 0.43));
    }
    b.obstacle(V(4.35, 1.5, -5.28), V(1.25, 1.5, 0.48));
    b.add(
        Shape::Robot,
        "ceramic",
        V(4.3, 1.2, -5.24),
        V(0.32, 0.32, 0.32),
        V(0., -18., 0.),
    );
    b.entity(
        "vesper-robot",
        "Vesper / little explorer",
        V(4.3, 1.6, -5.24),
        V(0.36, 0.45, 0.4),
    );
    for i in 0..7 {
        b.cube(
            if i % 2 == 0 { "blue" } else { "paper" },
            V(3.5 + i as f32 * 0.15, 0.6, -5.25),
            V(0.055, 0.30 + (i % 3) as f32 * 0.025, 0.24),
        );
    }
    b.add(
        Shape::Sphere,
        "brass",
        V(4.9, 2.49, -5.3),
        V(0.26, 0.26, 0.26),
        V::ZERO,
    );
    b.add(
        Shape::Cone,
        "ceramic",
        V(3.6, 2.43, -5.3),
        V(0.18, 0.23, 0.18),
        V::ZERO,
    );
    // Floor plants soften the corners.
    for (x, z) in [(-4.75, 3.75), (4.9, 4.65)] {
        b.add(
            Shape::Cylinder,
            "ceramic",
            V(x, 0.30, z),
            V(0.30, 0.30, 0.30),
            V::ZERO,
        );
        b.cube("oak", V(x, 0.9, z), V(0.025, 0.65, 0.025));
        for i in 0..9 {
            let a = i as f32 * 2.4;
            let y = 0.85 + i as f32 * 0.095;
            b.add(
                Shape::Sphere,
                "green",
                V(x + a.cos() * 0.23, y, z + a.sin() * 0.23),
                V(0.32, 0.10, 0.14),
                V(15., -a.to_degrees(), 28.),
            );
        }
        b.obstacle(V(x, 0.5, z), V(0.43, 0.5, 0.43));
    }
    // Entry door at the rear; intentionally closed in this single-room prototype.
    b.cube("trim", V(-1.8, 1.2, 5.83), V(0.75, 1.2, 0.05));
    b.cube("navy", V(-1.8, 1.18, 5.76), V(0.69, 1.17, 0.025));
    b.cube("brass", V(-1.27, 1.10, 5.69), V(0.035, 0.15, 0.035));
    b.entity(
        "entry",
        "Studio entrance",
        V(-1.8, 1.2, 5.75),
        V(0.8, 1.25, 0.2),
    );
    super::props::populate(&mut b);
    let compiled = Compiled::new(b.scene, Path::new("."))?;
    let world = compiled.at(0.);
    Ok(Room {
        name: "Studio".into(),
        simple_geometry: false,
        compiled,
        world,
        dynamic_world: World::new(vec![]),
        colliders: b.colliders,
        entities: b.entities,
        spatial: None,
    }
    .with_furniture_colliders())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::controller::Controller;
    #[test]
    fn room_is_valid_and_spawn_is_clear() {
        let r = build().unwrap();
        assert!(!r
            .colliders
            .iter()
            .any(|c| c.blocks(Controller::default().position)));
        assert!(r.world.instances.len() > 200);
    }
    #[test]
    fn room_contains_player_after_extended_walk() {
        let r = build().unwrap();
        let mut c = Controller::default();
        for i in 0..6000 {
            c.yaw = i as f32 * 0.01;
            c.step(1., 0.3, true, 1. / 60., &r.colliders);
            assert!(c.position.0.abs() < 5.8 && c.position.2.abs() < 5.8);
        }
    }
    #[test]
    fn stable_entity_ids_are_unique() {
        let r = build().unwrap();
        let ids: std::collections::HashSet<_> = r.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), r.entities.len());
    }
}
