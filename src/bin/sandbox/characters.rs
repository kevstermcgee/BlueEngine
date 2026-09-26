//! Cosmetic skins sharing the unmodified Scientist controller/hull.
//! Meshes are cached; only rigid limb transforms change while walking.
use super::wrench_view::box_part;
use macroquad::prelude::*;
use vesper3d::viewer::controller::Controller;

pub const NAMES: [&str; 8] = [
    "Scientist",
    "Feta",
    "Dusty Trails",
    "Nova Visitor",
    "Bolt-7",
    "Orbit Scout",
    "Brass Diver",
    "Cedar Ranger",
];
pub const IDS: [&str; 8] = [
    "scientist",
    "feta",
    "cowboy",
    "alien",
    "robot",
    "astronaut",
    "diver",
    "ranger",
];
pub const DESCRIPTIONS: [&str; 8] = [
    "The original lab-coated Scientist. Human movement and a 1.80 m collision hull.",
    "The original white rat, Feta. A 0.30 m body for testing furniture and tiny passages.",
    "Dusty Trails: a wide-brimmed hat, red bandana, brass sheriff badge, leather vest and chunky boots. Human movement and collision.",
    "Nova Visitor: a mint-green explorer with a big curious gaze, antennae and a violet survey suit. Human movement and collision.",
    "Bolt-7: a friendly maintenance robot with a cyan visor, amber chest panel, antenna and articulated steel limbs. Human movement and collision.",
    "Orbit Scout: a cream pressure suit, rounded helmet, dark blue visor and life-support backpack. Human movement and collision.",
    "Brass Diver: a vintage copper diving helmet, circular porthole, deep-blue suit and twin air tanks. Human movement and collision.",
    "Cedar Ranger: a forest-green jacket, broad field hat, canvas pack and binoculars. Human movement and collision.",
];
struct Part {
    mesh: Mesh,
    pivot: Vec3,
    swing: f32,
}
pub struct Skin {
    parts: Vec<Part>,
    posed: Mesh,
    phase: f32,
    stride: f32,
}
fn mesh() -> Mesh {
    Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    }
}
fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
fn ball(m: &mut Mesh, center: Vec3, radius: Vec3, color: Color) {
    let base = m.vertices.len() as u16;
    for j in 0..=8 {
        let a = j as f32 * std::f32::consts::PI / 8.;
        for i in 0..=12 {
            let b = i as f32 * std::f32::consts::TAU / 12.;
            let n = vec3(a.sin() * b.cos(), a.cos(), a.sin() * b.sin());
            let shade = 0.72 + 0.28 * n.dot(vec3(-0.4, 0.8, -0.3).normalize()).max(0.);
            m.vertices.push(Vertex::new2(
                center + n * radius,
                Vec2::ZERO,
                Color::new(color.r * shade, color.g * shade, color.b * shade, 1.),
            ));
        }
    }
    for j in 0..8 {
        for i in 0..12 {
            let a = base + j * 13 + i;
            m.indices
                .extend_from_slice(&[a, a + 1, a + 13, a + 1, a + 14, a + 13]);
        }
    }
}
fn cone(m: &mut Mesh, base: Vec3, radius: f32, height: f32, color: Color) {
    for i in 0..16 {
        let a = i as f32 * std::f32::consts::TAU / 16.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 16.;
        let offset = m.vertices.len() as u16;
        let c = Color::new(
            color.r * (0.75 + 0.2 * a.cos()),
            color.g * (0.75 + 0.2 * a.cos()),
            color.b * (0.75 + 0.2 * a.cos()),
            1.,
        );
        for p in [
            base + vec3(a.cos() * radius, 0., a.sin() * radius),
            base + vec3(0.08, height, 0.),
            base + vec3(b.cos() * radius, 0., b.sin() * radius),
        ] {
            m.vertices.push(Vertex::new2(p, Vec2::ZERO, c));
        }
        m.indices
            .extend_from_slice(&[offset, offset + 1, offset + 2]);
    }
}
fn star(m: &mut Mesh, center: Vec3, radius: f32, color: Color) {
    for i in 0..10 {
        let point = |j: usize| {
            let angle = j as f32 * std::f32::consts::TAU / 10. + std::f32::consts::FRAC_PI_2;
            let r = if j.is_multiple_of(2) {
                radius
            } else {
                radius * 0.43
            };
            center + vec3(angle.cos() * r, angle.sin() * r, 0.)
        };
        let base = m.vertices.len() as u16;
        for p in [center, point(i), point(i + 1)] {
            m.vertices.push(Vertex::new2(p, Vec2::ZERO, color));
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
}
impl Skin {
    pub fn new(index: usize) -> Self {
        // Keep the retired wizard model available for later without listing it.
        let index = [0, 1, 3, 4, 5, 6, 7, 8][index];
        let mut parts = vec![];
        let mut torso = mesh();
        let cloth = match index {
            2 => rgb(60, 47, 125),
            3 => rgb(105, 61, 35),
            4 => rgb(96, 69, 153),
            6 => rgb(225, 228, 216),
            7 => rgb(38, 68, 98),
            8 => rgb(59, 103, 69),
            _ => rgb(53, 102, 121),
        };
        let skin = match index {
            4 => rgb(131, 219, 164),
            5 => rgb(179, 195, 193),
            _ => rgb(212, 160, 115),
        };
        let gold = rgb(243, 192, 67);
        let black = rgb(23, 30, 42);
        let white = rgb(220, 232, 226);
        box_part(
            &mut torso,
            vec3(0., 1.13, 0.),
            vec3(0.47, 0.52, 0.29),
            cloth,
        );
        box_part(
            &mut torso,
            vec3(0., 0.90, 0.),
            vec3(0.48, 0.065, 0.31),
            black,
        );
        box_part(
            &mut torso,
            vec3(0., 0.90, -0.17),
            vec3(0.09, 0.08, 0.03),
            gold,
        );
        ball(
            &mut torso,
            vec3(0., 1.60, 0.),
            if index == 4 {
                vec3(0.23, 0.25, 0.18)
            } else {
                vec3(0.16, 0.20, 0.15)
            },
            skin,
        );
        for x in [-0.065, 0.065] {
            ball(
                &mut torso,
                vec3(x, 1.63, -0.142),
                if index == 4 {
                    vec3(0.058, 0.083, 0.03)
                } else {
                    vec3(0.025, 0.035, 0.02)
                },
                black,
            );
        }
        match index {
            2 => {
                cone(&mut torso, vec3(0., 0.43, 0.), 0.34, 0.68, cloth);
                ball(
                    &mut torso,
                    vec3(0., 1.79, 0.),
                    vec3(0.35, 0.04, 0.30),
                    cloth,
                );
                cone(&mut torso, vec3(0., 1.81, 0.), 0.24, 0.55, cloth);
                star(&mut torso, vec3(0.08, 2.38, -0.01), 0.09, gold);
                cone(&mut torso, vec3(0., 1.51, -0.17), 0.10, -0.28, white);
                for y in [0.65, 0.83, 1.05, 1.24] {
                    ball(
                        &mut torso,
                        vec3(0., y, -0.165),
                        vec3(0.028, 0.028, 0.025),
                        gold,
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0.44, 1.07, -0.10),
                    vec3(0.045, 1.65, 0.045),
                    rgb(119, 75, 43),
                );
                ball(
                    &mut torso,
                    vec3(0.44, 1.95, -0.1),
                    vec3(0.09, 0.13, 0.09),
                    rgb(92, 224, 241),
                );
            }
            3 => {
                ball(
                    &mut torso,
                    vec3(0., 1.79, 0.),
                    vec3(0.36, 0.045, 0.28),
                    rgb(148, 101, 50),
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.89, 0.02),
                    vec3(0.30, 0.18, 0.25),
                    rgb(148, 101, 50),
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.80, 0.),
                    vec3(0.31, 0.055, 0.27),
                    black,
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.38, -0.09),
                    vec3(0.29, 0.10, 0.22),
                    rgb(181, 49, 40),
                );
                star(&mut torso, vec3(-0.12, 1.22, -0.165), 0.073, gold);
                for x in [-0.10, 0.10] {
                    box_part(
                        &mut torso,
                        vec3(x, 1.11, -0.16),
                        vec3(0.10, 0.16, 0.015),
                        rgb(75, 45, 28),
                    );
                }
            }
            4 => {
                for x in [-0.14, 0.14] {
                    box_part(&mut torso, vec3(x, 1.91, 0.), vec3(0.02, 0.25, 0.02), skin);
                    ball(
                        &mut torso,
                        vec3(x, 2.05, 0.),
                        vec3(0.045, 0.055, 0.045),
                        gold,
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0., 1.12, -0.17),
                    vec3(0.27, 0.22, 0.055),
                    rgb(198, 216, 224),
                );
                for x in [-0.07, 0., 0.07] {
                    ball(
                        &mut torso,
                        vec3(x, 1.13, -0.21),
                        vec3(0.021, 0.021, 0.015),
                        rgb(77, 195, 207),
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0., 1.17, 0.24),
                    vec3(0.33, 0.39, 0.18),
                    rgb(207, 182, 78),
                );
            }
            6 => {
                ball(
                    &mut torso,
                    vec3(0., 1.66, 0.),
                    vec3(0.255, 0.265, 0.235),
                    white,
                );
                ball(
                    &mut torso,
                    vec3(0., 1.67, -0.19),
                    vec3(0.195, 0.17, 0.09),
                    rgb(29, 61, 89),
                );
                box_part(
                    &mut torso,
                    vec3(-0.07, 1.74, -0.27),
                    vec3(0.09, 0.025, 0.014),
                    rgb(113, 215, 233),
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.12, -0.18),
                    vec3(0.28, 0.22, 0.06),
                    rgb(81, 105, 118),
                );
                for x in [-0.08, 0., 0.08] {
                    ball(
                        &mut torso,
                        vec3(x, 1.13, -0.225),
                        vec3(0.025, 0.025, 0.018),
                        gold,
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0., 1.14, 0.24),
                    vec3(0.36, 0.52, 0.20),
                    white,
                );
                for x in [-0.19, 0.19] {
                    box_part(
                        &mut torso,
                        vec3(x, 1.17, 0.27),
                        vec3(0.06, 0.40, 0.12),
                        rgb(198, 90, 46),
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0., 0.97, -0.18),
                    vec3(0.47, 0.055, 0.04),
                    rgb(198, 90, 46),
                );
            }
            7 => {
                let brass = rgb(188, 131, 63);
                ball(
                    &mut torso,
                    vec3(0., 1.65, 0.),
                    vec3(0.27, 0.265, 0.25),
                    brass,
                );
                ball(
                    &mut torso,
                    vec3(0., 1.64, -0.22),
                    vec3(0.18, 0.18, 0.06),
                    gold,
                );
                ball(
                    &mut torso,
                    vec3(0., 1.64, -0.265),
                    vec3(0.135, 0.135, 0.025),
                    rgb(26, 72, 88),
                );
                for x in [-0.075, 0., 0.075] {
                    box_part(
                        &mut torso,
                        vec3(x, 1.64, -0.29),
                        vec3(0.018, 0.21, 0.012),
                        brass,
                    );
                }
                ball(
                    &mut torso,
                    vec3(0., 1.40, 0.),
                    vec3(0.30, 0.065, 0.27),
                    brass,
                );
                for x in [-0.12, 0.12] {
                    ball(
                        &mut torso,
                        vec3(x, 1.11, 0.27),
                        vec3(0.10, 0.32, 0.11),
                        rgb(111, 132, 123),
                    );
                    box_part(
                        &mut torso,
                        vec3(x, 1.16, -0.17),
                        vec3(0.045, 0.44, 0.03),
                        brass,
                    );
                }
            }
            8 => {
                let canvas = rgb(167, 148, 95);
                ball(
                    &mut torso,
                    vec3(0., 1.79, 0.),
                    vec3(0.34, 0.035, 0.29),
                    canvas,
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.87, 0.),
                    vec3(0.30, 0.15, 0.25),
                    canvas,
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.80, 0.),
                    vec3(0.31, 0.04, 0.26),
                    rgb(68, 61, 38),
                );
                box_part(
                    &mut torso,
                    vec3(0., 1.15, 0.26),
                    vec3(0.40, 0.45, 0.22),
                    canvas,
                );
                for x in [-0.15, 0.15] {
                    box_part(
                        &mut torso,
                        vec3(x, 1.17, -0.16),
                        vec3(0.035, 0.42, 0.025),
                        canvas,
                    );
                    box_part(
                        &mut torso,
                        vec3(x, 1.05, -0.20),
                        vec3(0.10, 0.12, 0.03),
                        canvas,
                    );
                }
                for x in [-0.055, 0.055] {
                    ball(
                        &mut torso,
                        vec3(x, 1.24, -0.22),
                        vec3(0.055, 0.08, 0.055),
                        black,
                    );
                }
                star(&mut torso, vec3(-0.14, 1.31, -0.18), 0.045, gold);
            }
            _ => {
                box_part(&mut torso, vec3(0., 1.62, 0.), vec3(0.38, 0.32, 0.30), skin);
                box_part(
                    &mut torso,
                    vec3(0., 1.66, -0.16),
                    vec3(0.29, 0.11, 0.03),
                    black,
                );
                for x in [-0.075, 0.075] {
                    box_part(
                        &mut torso,
                        vec3(x, 1.66, -0.18),
                        vec3(0.045, 0.055, 0.015),
                        rgb(82, 227, 242),
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0., 1.12, -0.17),
                    vec3(0.30, 0.27, 0.055),
                    gold,
                );
                for y in [1.05, 1.11, 1.17] {
                    box_part(
                        &mut torso,
                        vec3(0., y, -0.205),
                        vec3(0.19, 0.018, 0.02),
                        black,
                    );
                }
                box_part(
                    &mut torso,
                    vec3(0.11, 1.88, 0.),
                    vec3(0.025, 0.18, 0.025),
                    skin,
                );
                ball(
                    &mut torso,
                    vec3(0.11, 1.99, 0.),
                    vec3(0.046, 0.046, 0.046),
                    rgb(224, 88, 48),
                );
            }
        }
        parts.push(Part {
            mesh: torso,
            pivot: Vec3::ZERO,
            swing: 0.,
        });
        for side in [-1., 1.] {
            let mut leg = mesh();
            let x = side * 0.13;
            box_part(
                &mut leg,
                vec3(x, 0.52, 0.),
                vec3(0.17, 0.64, 0.19),
                if index == 3 { rgb(44, 70, 103) } else { cloth },
            );
            box_part(
                &mut leg,
                vec3(x, 0.14, -0.06),
                vec3(0.21, 0.27, 0.32),
                black,
            );
            if index == 5 {
                ball(&mut leg, vec3(x, 0.46, -0.02), vec3(0.12, 0.10, 0.12), gold);
            }
            parts.push(Part {
                mesh: leg,
                pivot: vec3(x, 0.84, 0.),
                swing: side,
            });
            let mut arm = mesh();
            let x = side * 0.32;
            ball(&mut arm, vec3(x, 1.29, 0.), vec3(0.115, 0.13, 0.13), cloth);
            box_part(
                &mut arm,
                vec3(x, 1.07, 0.),
                vec3(0.15, 0.40, 0.17),
                if index == 3 {
                    rgb(195, 174, 126)
                } else {
                    cloth
                },
            );
            ball(&mut arm, vec3(x, 0.82, 0.), vec3(0.085, 0.105, 0.075), skin);
            parts.push(Part {
                mesh: arm,
                pivot: vec3(x, 1.30, 0.),
                swing: -side * 0.7,
            });
        }
        Self {
            parts,
            posed: mesh(),
            phase: 0.,
            stride: 0.,
        }
    }
    pub fn update(&mut self, distance: f32, dt: f32, moving: bool) {
        self.phase += distance * 8.;
        let target = if moving { 0.65 } else { 0. };
        self.stride += (target - self.stride) * (1. - (-12. * dt).exp());
    }
    pub fn draw(&mut self, player: &Controller) {
        self.posed.vertices.clear();
        self.posed.indices.clear();
        let root = Mat4::from_scale_rotation_translation(
            vec3(1., player.body_height() / 1.8, 1.),
            Quat::from_rotation_y(-player.yaw),
            vec3(player.position.0, player.feet_height(), player.position.2),
        );
        for part in &self.parts {
            let transform = root
                * Mat4::from_translation(part.pivot)
                * Mat4::from_rotation_x(self.phase.sin() * self.stride * part.swing)
                * Mat4::from_translation(-part.pivot);
            let base = self.posed.vertices.len() as u16;
            for v in &part.mesh.vertices {
                let mut v = *v;
                v.position = transform.transform_point3(v.position);
                self.posed.vertices.push(v);
            }
            self.posed
                .indices
                .extend(part.mesh.indices.iter().map(|i| i + base));
        }
        draw_mesh(&self.posed);
    }
}
