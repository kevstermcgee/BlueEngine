//! Default Blue mechanic skin, assembled from small rigid mesh parts.
use super::wrench_view::{box_part, swing_amount, View};
use macroquad::prelude::*;
use vesper3d::viewer::{controller::Controller, wrench::Wrench};
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
    pub fn draw(&mut self, player: &Controller, wrench: &Wrench, tool: &View) {
        let blue = Color::new(0.035, 0.20, 0.49, 1.);
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
        // Jacket zipper, chest pocket, shoulder bands and back emblem.
        box_part(
            &mut m,
            vec3(0., chest, -0.154),
            vec3(0.014, 0.45, 0.012),
            trim,
        );
        box_part(
            &mut m,
            vec3(-0.13, chest + 0.06, -0.162),
            vec3(0.12, 0.13, 0.025),
            navy,
        );
        box_part(
            &mut m,
            vec3(0.13, chest + 0.08, -0.162),
            vec3(0.09, 0.035, 0.025),
            trim,
        );
        box_part(
            &mut m,
            vec3(0., chest + 0.12, 0.151),
            vec3(0.36, 0.06, 0.025),
            trim,
        );
        box_part(
            &mut m,
            vec3(0., chest - 0.04, 0.157),
            vec3(0.15, 0.18, 0.028),
            navy,
        );
        box_part(
            &mut m,
            vec3(0., chest - 0.04, 0.176),
            vec3(0.04, 0.12, 0.016),
            trim,
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
        let swing = swing_amount(wrench);
        let shoulder = vec3(0.30, chest + 0.13, 0.);
        let hand = vec3(
            0.31 - 0.23 * swing,
            chest - 0.12 + 0.22 * swing,
            -0.26 - 0.34 * swing,
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
            Quat::from_rotation_z(0.10 + 0.75 * swing) * Quat::from_rotation_x(-0.25 - 1.3 * swing),
            hand,
        );
        tool.draw_held(root * wrist);
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
