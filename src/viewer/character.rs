//! Procedural Scientist and Feta skins, with distance-driven animation.
use super::wrench_view::{box_part, swing_amount, View};
use crate::viewer::{
    controller::{CharacterKind, Controller},
    wrench::Wrench,
};
use macroquad::prelude::*;
#[derive(Default)]
pub struct Character {
    phase: f32,
    stride: f32,
    mesh: Option<Mesh>,
}
impl Character {
    pub fn update(&mut self, distance: f32, dt: f32, moving: bool) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        self.phase = (self.phase + distance * 8.).rem_euclid(std::f32::consts::TAU);
        let target = if moving {
            (distance / dt / 3.2).clamp(0., 1.3)
        } else {
            0.
        };
        self.stride += (target - self.stride) * (1. - (-16. * dt).exp());
    }
    fn draw_feta(&mut self, player: &Controller) {
        let mut m = self.mesh.take().unwrap_or(Mesh {
            vertices: Vec::with_capacity(6000),
            indices: Vec::with_capacity(12000),
            texture: None,
        });
        m.vertices.clear();
        m.indices.clear();
        let fur = Color::new(0.94, 0.94, 0.90, 1.);
        let pink = Color::new(0.84, 0.51, 0.51, 1.);
        let red = Color::new(0.65, 0.025, 0.055, 1.);
        let gait = self.phase.sin() * self.stride * 0.035;
        ellipsoid(&mut m, vec3(0., 0.15, 0.06), vec3(0.125, 0.125, 0.22), fur);
        ellipsoid(&mut m, vec3(0., 0.19, -0.16), vec3(0.096, 0.09, 0.13), fur);
        ellipsoid(
            &mut m,
            vec3(0., 0.155, -0.265),
            vec3(0.058, 0.045, 0.075),
            fur,
        );
        ellipsoid(
            &mut m,
            vec3(0., 0.16, -0.331),
            vec3(0.024, 0.019, 0.018),
            pink,
        );
        for side in [-1., 1.] {
            ellipsoid(
                &mut m,
                vec3(side * 0.084, 0.268, -0.115),
                vec3(0.05, 0.064, 0.025),
                fur,
            );
            ellipsoid(
                &mut m,
                vec3(side * 0.084, 0.271, -0.134),
                vec3(0.036, 0.047, 0.012),
                pink,
            );
            ellipsoid(
                &mut m,
                vec3(side * 0.078, 0.211, -0.219),
                vec3(0.024, 0.026, 0.021),
                red,
            );
            ellipsoid(
                &mut m,
                vec3(side * 0.083, 0.221, -0.232),
                vec3(0.006, 0.007, 0.006),
                WHITE,
            );
            for (z, step) in [(-0.12, gait * side), (0.18, -gait * side)] {
                ellipsoid(
                    &mut m,
                    vec3(side * 0.105, 0.065, z + step),
                    vec3(0.033, 0.061, 0.043),
                    fur,
                );
                ellipsoid(
                    &mut m,
                    vec3(side * 0.11, 0.025, z - 0.025 + step),
                    vec3(0.038, 0.021, 0.055),
                    pink,
                );
                for toe in [-1., 0., 1.] {
                    segment(
                        &mut m,
                        vec3(side * 0.11 + toe * 0.012, 0.021, z - 0.04 + step),
                        vec3(side * 0.11 + toe * 0.018, 0.017, z - 0.084 + step),
                        0.008,
                        0.008,
                        pink,
                    );
                }
            }
            for i in 0..3 {
                segment(
                    &mut m,
                    vec3(side * 0.025, 0.16, -0.29),
                    vec3(
                        side * 0.16,
                        0.155 + i as f32 * 0.015,
                        -0.32 + i as f32 * 0.033,
                    ),
                    0.002,
                    0.002,
                    Color::new(0.7, 0.7, 0.67, 1.),
                );
            }
        }
        let mut tail = vec3(0., 0.10, 0.23);
        for i in 1..13 {
            let t = i as f32 / 12.;
            let next = vec3(
                0.10 * (t * 3. + self.phase * 0.3).sin() * t,
                0.035 + 0.065 * (1. - t),
                0.23 + t * 0.39,
            );
            segment(
                &mut m,
                tail,
                next,
                0.025 * (1. - t) + 0.006,
                0.025 * (1. - t) + 0.006,
                pink,
            );
            tail = next;
        }
        let root = Mat4::from_scale_rotation_translation(
            vec3(1., player.body_height() / 0.30, 1.),
            Quat::from_rotation_y(-player.yaw),
            vec3(player.position.0, player.feet_height(), player.position.2),
        );
        for v in &mut m.vertices {
            v.position = root.transform_point3(v.position);
        }
        draw_mesh(&m);
        self.mesh = Some(m);
    }
    pub fn draw(&mut self, player: &Controller, wrench: &Wrench, tool: &View, carrying: bool) {
        if player.character_kind() == CharacterKind::Feta {
            self.draw_feta(player);
            return;
        }
        let blue = Color::new(0.92, 0.95, 0.96, 1.);
        let navy = Color::new(0.055, 0.085, 0.13, 1.);
        let skin = Color::new(0.69, 0.45, 0.29, 1.);
        let boot = Color::new(0.07, 0.055, 0.045, 1.);
        let trim = Color::new(0.65, 0.79, 0.88, 1.);
        let mut m = self.mesh.take().unwrap_or(Mesh {
            vertices: Vec::with_capacity(1600),
            indices: Vec::with_capacity(2400),
            texture: None,
        });
        m.vertices.clear();
        m.indices.clear();
        let crouch = ((1.8 - player.body_height()) / 0.7).clamp(0., 1.);
        let hip = 0.91 - 0.47 * crouch;
        let chest = 1.25 - 0.63 * crouch;
        let gait = if player.is_grounded() {
            self.phase.sin() * self.stride * 0.25
        } else {
            0.12
        };
        // Bent knees lower the pelvis while keeping boots at ground level.
        for (side, step) in [(-1., gait), (1., -gait)] {
            let x = side * 0.125;
            let knee = vec3(x, 0.49 - 0.20 * crouch, -0.24 * crouch + step * 0.5);
            let foot = vec3(x, 0.12, step);
            segment(&mut m, vec3(x, hip, 0.), knee, 0.19, 0.20, navy);
            segment(&mut m, knee, foot, 0.16, 0.17, navy);
            box_part(
                &mut m,
                foot + vec3(0., -0.035, -0.065),
                vec3(0.20, 0.17, 0.32),
                boot,
            );
            box_part(
                &mut m,
                knee + vec3(0., 0., -0.10),
                vec3(0.14, 0.15, 0.035),
                trim,
            );
        }
        box_part(&mut m, vec3(0., hip, 0.), vec3(0.43, 0.22, 0.27), navy);
        box_part(&mut m, vec3(0., chest, 0.), vec3(0.48, 0.51, 0.29), blue);
        box_part(
            &mut m,
            vec3(0., hip + 0.08, -0.01),
            vec3(0.46, 0.075, 0.31),
            boot,
        );
        box_part(
            &mut m,
            vec3(0., hip + 0.08, -0.175),
            vec3(0.085, 0.065, 0.025),
            trim,
        );
        // Knee-length split coat, lapels, pockets, buttons and pen.
        for side in [-1., 1.] {
            box_part(
                &mut m,
                vec3(side * 0.126, hip - 0.12, 0.),
                vec3(0.245, 0.49, 0.34),
                blue,
            );
            box_part(
                &mut m,
                vec3(side * 0.13, chest - 0.15, -0.17),
                vec3(0.14, 0.13, 0.025),
                trim,
            );
            segment(
                &mut m,
                vec3(side * 0.15, chest + 0.23, -0.165),
                vec3(side * 0.035, chest + 0.03, -0.18),
                0.065,
                0.025,
                WHITE,
            );
        }
        for dy in [-0.18, -0.04, 0.10] {
            box_part(
                &mut m,
                vec3(0., chest + dy, -0.165),
                vec3(0.023, 0.023, 0.014),
                navy,
            );
        }
        box_part(
            &mut m,
            vec3(-0.14, chest + 0.08, -0.17),
            vec3(0.115, 0.12, 0.025),
            trim,
        );
        box_part(
            &mut m,
            vec3(-0.16, chest + 0.15, -0.19),
            vec3(0.018, 0.10, 0.014),
            navy,
        );
        let head_y = player.body_height() - 0.19;
        box_part(
            &mut m,
            vec3(0., head_y - 0.20, 0.),
            vec3(0.14, 0.15, 0.15),
            skin,
        );
        let head_start = m.vertices.len();
        box_part(&mut m, vec3(0., head_y, 0.), vec3(0.29, 0.32, 0.28), skin);
        let hair = Color::new(0.10, 0.065, 0.045, 1.);
        box_part(
            &mut m,
            vec3(0., head_y + 0.135, 0.015),
            vec3(0.31, 0.085, 0.30),
            hair,
        );
        box_part(
            &mut m,
            vec3(0., head_y + 0.015, 0.135),
            vec3(0.31, 0.20, 0.055),
            hair,
        );
        for x in [-0.068, 0.068] {
            box_part(
                &mut m,
                vec3(x, head_y + 0.012, -0.148),
                vec3(0.065, 0.045, 0.025),
                trim,
            );
            box_part(
                &mut m,
                vec3(x, head_y + 0.009, -0.165),
                vec3(0.026, 0.03, 0.016),
                navy,
            );
            box_part(
                &mut m,
                vec3(x, head_y + 0.056, -0.146),
                vec3(0.074, 0.019, 0.018),
                hair,
            );
        }
        box_part(
            &mut m,
            vec3(0., head_y - 0.032, -0.16),
            vec3(0.045, 0.065, 0.065),
            skin,
        );
        box_part(
            &mut m,
            vec3(0., head_y - 0.096, -0.146),
            vec3(0.10, 0.018, 0.013),
            hair,
        );
        // Dark rectangular eyeglass rims, bridge and temples.
        for x in [-0.078, 0.078] {
            for dy in [-0.028, 0.052] {
                box_part(
                    &mut m,
                    vec3(x, head_y + dy, -0.184),
                    vec3(0.125, 0.014, 0.018),
                    navy,
                );
            }
            for dx in [-0.058, 0.058] {
                box_part(
                    &mut m,
                    vec3(x + dx, head_y + 0.012, -0.184),
                    vec3(0.013, 0.08, 0.018),
                    navy,
                );
            }
            box_part(
                &mut m,
                vec3(x.signum() * 0.147, head_y + 0.025, -0.06),
                vec3(0.015, 0.018, 0.24),
                navy,
            );
        }
        box_part(
            &mut m,
            vec3(0., head_y + 0.025, -0.184),
            vec3(0.04, 0.014, 0.018),
            navy,
        );
        let head_rot = Quat::from_rotation_x(player.pitch.clamp(-0.6, 0.6));
        for v in &mut m.vertices[head_start..] {
            v.position = vec3(0., head_y, 0.) + head_rot * (v.position - vec3(0., head_y, 0.));
        }
        let left_shoulder = vec3(-0.30, chest + 0.13, 0.);
        let left_elbow = left_shoulder + vec3(-0.015, -0.26, -gait * 0.5);
        let left_hand = left_elbow + vec3(0., -0.24, -0.05 - gait * 0.5);
        segment(&mut m, left_shoulder, left_elbow, 0.17, 0.20, blue);
        segment(&mut m, left_elbow, left_hand, 0.14, 0.16, blue);
        box_part(&mut m, left_hand, vec3(0.13, 0.15, 0.13), skin);
        let swing = if tool.is_pistol() {
            0.
        } else {
            swing_amount(wrench)
        };
        let shoulder = vec3(0.30, chest + 0.13, 0.);
        let hand = vec3(
            0.31 - 0.23 * swing,
            chest - 0.12 + 0.22 * swing + if tool.is_pistol() { 0.2 } else { 0. },
            -0.26 - 0.34 * swing - if tool.is_pistol() { 0.16 } else { 0. },
        );
        let elbow = shoulder.lerp(hand, 0.52) + vec3(0.06, -0.12, 0.07);
        segment(&mut m, shoulder, elbow, 0.17, 0.20, blue);
        segment(&mut m, elbow, hand, 0.14, 0.16, blue);
        let root = Mat4::from_rotation_translation(
            Quat::from_rotation_y(-player.yaw),
            vec3(player.position.0, player.feet_height(), player.position.2),
        );
        for v in &mut m.vertices {
            v.position = root.transform_point3(v.position);
        }
        draw_mesh(&m);
        self.mesh = Some(m);
        let wrist = Mat4::from_rotation_translation(
            if tool.is_pistol() {
                Quat::from_rotation_x(player.pitch)
            } else {
                Quat::from_rotation_z(0.10 + 0.75 * swing)
                    * Quat::from_rotation_x(-0.25 - 1.3 * swing)
            },
            hand,
        );
        if !carrying {
            tool.draw_held(root * wrist);
        }
    }
}
fn segment(m: &mut Mesh, a: Vec3, b: Vec3, w: f32, d: f32, c: Color) {
    let start = m.vertices.len();
    let length = (b - a).length();
    box_part(m, Vec3::ZERO, vec3(w, length, d), c);
    let rot = Quat::from_rotation_arc(Vec3::Y, (b - a).normalize());
    for v in &mut m.vertices[start..] {
        v.position = rot * v.position + (a + b) * 0.5;
    }
}

fn ellipsoid(m: &mut Mesh, center: Vec3, radii: Vec3, color: Color) {
    let base = m.vertices.len() as u16;
    for row in 0..=8 {
        let latitude = std::f32::consts::PI * row as f32 / 8.;
        for col in 0..=12 {
            let longitude = std::f32::consts::TAU * col as f32 / 12.;
            let n = vec3(
                latitude.sin() * longitude.cos(),
                latitude.cos(),
                latitude.sin() * longitude.sin(),
            );
            let shade = 0.72 + 0.28 * n.dot(vec3(-0.3, 0.8, -0.5).normalize()).max(0.);
            m.vertices.push(Vertex::new(
                center.x + n.x * radii.x,
                center.y + n.y * radii.y,
                center.z + n.z * radii.z,
                0.,
                0.,
                Color::new(color.r * shade, color.g * shade, color.b * shade, 1.),
            ));
        }
    }
    for row in 0..8 {
        for col in 0..12 {
            let a = base + row * 13 + col;
            m.indices
                .extend_from_slice(&[a, a + 13, a + 1, a + 1, a + 13, a + 14]);
        }
    }
}
