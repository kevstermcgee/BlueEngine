use macroquad::prelude::*;
use vesper3d::viewer::wrench::Wrench;

// A small dedicated depth buffer keeps the held tool out of room geometry.
pub struct View {
    mesh: Mesh,
    held: Mesh,
    target: RenderTarget,
    posed: Mesh,
    posed_held: std::cell::RefCell<Mesh>,
}
impl View {
    pub fn new() -> Self {
        let mut mesh = Mesh {
            vertices: vec![],
            indices: vec![],
            texture: None,
        };
        let steel = Color::new(0.64, 0.74, 0.84, 1.);
        let grip = Color::new(0.06, 0.12, 0.20, 1.);
        box_part(
            &mut mesh,
            vec3(0., 0.16, 0.),
            vec3(0.065, 0.52, 0.055),
            steel,
        );
        box_part(
            &mut mesh,
            vec3(0., -0.015, 0.),
            vec3(0.083, 0.20, 0.072),
            grip,
        );
        // Open-ended jaws: the gap remains visibly open.
        box_part(
            &mut mesh,
            vec3(0., 0.43, 0.),
            vec3(0.22, 0.09, 0.075),
            steel,
        );
        box_part(
            &mut mesh,
            vec3(-0.086, 0.505, 0.),
            vec3(0.065, 0.15, 0.075),
            steel,
        );
        box_part(
            &mut mesh,
            vec3(0.086, 0.505, 0.),
            vec3(0.065, 0.15, 0.075),
            steel,
        );
        box_part(
            &mut mesh,
            vec3(0., -0.035, 0.018),
            vec3(0.135, 0.13, 0.12),
            Color::new(0.46, 0.27, 0.16, 1.),
        );
        for y in [-0.075, -0.041, -0.007, 0.027] {
            box_part(
                &mut mesh,
                vec3(-0.005, y, -0.055),
                vec3(0.12, 0.026, 0.045),
                Color::new(0.69, 0.45, 0.29, 1.),
            );
        }
        let held = Mesh {
            vertices: mesh.vertices.clone(),
            indices: mesh.indices.clone(),
            texture: None,
        };
        box_part(
            &mut mesh,
            vec3(0.01, -0.29, 0.07),
            vec3(0.15, 0.39, 0.16),
            Color::new(0.035, 0.20, 0.49, 1.),
        );
        let posed = Mesh {
            vertices: mesh.vertices.clone(),
            indices: mesh.indices.clone(),
            texture: None,
        };
        let posed_held = std::cell::RefCell::new(Mesh {
            vertices: held.vertices.clone(),
            indices: held.indices.clone(),
            texture: None,
        });
        Self {
            posed,
            posed_held,
            mesh,
            held,
            target: render_target(1, 1),
        }
    }
    pub fn draw_held(&self, transform: Mat4) {
        let mut posed = self.posed_held.borrow_mut();
        for (v, source) in posed.vertices.iter_mut().zip(&self.held.vertices) {
            v.position = transform.transform_point3(source.position);
        }
        draw_mesh(&posed);
    }
    pub fn draw(&mut self, wrench: &Wrench) {
        let (w, h) = (screen_width() as u32, screen_height() as u32);
        if self.target.texture.width() as u32 != w || self.target.texture.height() as u32 != h {
            self.target = render_target(w.max(1), h.max(1));
        }
        let swing = swing_amount(wrench);
        let rotation = Quat::from_rotation_z(0.20 + 1.15 * swing)
            * Quat::from_rotation_x(-0.18 - 0.65 * swing);
        let shift = vec3(0.40 - 0.24 * swing, -0.40 + 0.06 * swing, -0.90);
        for (v, source) in self.posed.vertices.iter_mut().zip(&self.mesh.vertices) {
            v.position = rotation * (source.position * 0.82) + shift;
        }
        set_camera(&Camera3D {
            position: Vec3::ZERO,
            target: vec3(0., 0., -1.),
            up: Vec3::Y,
            fovy: 65_f32.to_radians(),
            z_near: 0.01,
            z_far: 3.,
            render_target: Some(self.target.clone()),
            ..Default::default()
        });
        clear_background(Color::new(0., 0., 0., 0.));
        draw_mesh(&self.posed);
        set_default_camera();
        draw_texture_ex(
            &self.target.texture,
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w as f32, h as f32)),
                flip_y: true,
                ..Default::default()
            },
        );
    }
}
pub(super) fn box_part(mesh: &mut Mesh, center: Vec3, size: Vec3, color: Color) {
    let corners = [
        vec3(-1., -1., -1.),
        vec3(1., -1., -1.),
        vec3(1., 1., -1.),
        vec3(-1., 1., -1.),
        vec3(-1., -1., 1.),
        vec3(1., -1., 1.),
        vec3(1., 1., 1.),
        vec3(-1., 1., 1.),
    ];
    for (face, light) in [
        ([0, 3, 2, 1], 0.90),
        ([4, 5, 6, 7], 0.70),
        ([0, 4, 7, 3], 0.55),
        ([1, 2, 6, 5], 0.80),
        ([3, 7, 6, 2], 1.0),
        ([0, 1, 5, 4], 0.40),
    ] {
        let base = mesh.vertices.len() as u16;
        for i in face {
            let p = center + corners[i] * size * 0.5;
            mesh.vertices.push(Vertex::new(
                p.x,
                p.y,
                p.z,
                0.,
                0.,
                Color::new(color.r * light, color.g * light, color.b * light, 1.),
            ));
        }
        mesh.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

pub(super) fn swing_amount(wrench: &Wrench) -> f32 {
    let p = wrench.phase().unwrap_or(0.);
    let smooth = |t: f32| {
        let t = t.clamp(0., 1.);
        t * t * (3. - 2. * t)
    };
    if p < 0.20 {
        -0.25 * smooth(p / 0.20)
    } else if p < 0.40 {
        -0.25 + 1.25 * smooth((p - 0.20) / 0.20)
    } else {
        1. - smooth((p - 0.40) / 0.60)
    }
}
