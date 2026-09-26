use crate::viewer::wrench::Wrench;
use macroquad::prelude::*;

// A small dedicated depth buffer keeps the held tool out of room geometry.
pub struct View {
    pistol: bool,
    mesh: Mesh,
    held: Mesh,
    target: RenderTarget,
    posed: Mesh,
    posed_held: std::cell::RefCell<Mesh>,
}
impl View {
    pub fn new() -> Self {
        Self::build(false)
    }
    pub fn pistol() -> Self {
        Self::build(true)
    }
    pub fn is_pistol(&self) -> bool {
        self.pistol
    }
    fn build(pistol: bool) -> Self {
        let mut mesh = Mesh {
            vertices: vec![],
            indices: vec![],
            texture: None,
        };
        if pistol {
            black_pistol(&mut mesh);
        } else {
            forged_wrench(&mut mesh);
        }
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
            Color::new(0.92, 0.95, 0.96, 1.),
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
            pistol,
            posed,
            posed_held,
            mesh,
            held,
            target: tool_target(1, 1),
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
        let swing = swing_amount(wrench);
        let rotation = Quat::from_rotation_z(0.20 + 1.15 * swing)
            * Quat::from_rotation_x(-0.18 - 0.65 * swing);
        let shift = vec3(0.40 - 0.24 * swing, -0.40 + 0.06 * swing, -0.90);
        self.draw_pose(rotation, shift, false);
    }
    pub fn draw_pistol(&mut self, pistol: &crate::viewer::weapons::Pistol) {
        self.draw_pose(
            Quat::from_rotation_x(pistol.recoil() * 0.22),
            vec3(
                0.38,
                -0.33 + pistol.recoil() * 0.035,
                -0.90 + pistol.recoil() * 0.09,
            ),
            pistol.flash > 0.,
        );
    }
    fn draw_pose(&mut self, rotation: Quat, shift: Vec3, flash: bool) {
        let (w, h) = (screen_width() as u32, screen_height() as u32);
        if self.target.texture.width() as u32 != w || self.target.texture.height() as u32 != h {
            self.target = tool_target(w.max(1), h.max(1));
        }
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
        if flash {
            let muzzle = rotation * vec3(0., 0.07, -0.34) * 0.82 + shift;
            draw_sphere(muzzle, 0.032, None, Color::new(1., 0.78, 0.27, 1.));
            draw_line_3d(
                muzzle,
                muzzle + rotation * vec3(0., 0., -0.13),
                Color::new(1., 0.92, 0.65, 1.),
            );
        }
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

// Stylized compact black sidearm; visual mesh only, shared by both perspectives.
fn black_pistol(mesh: &mut Mesh) {
    let black = Color::new(0.025, 0.029, 0.035, 1.);
    let slide = Color::new(0.065, 0.075, 0.085, 1.);
    let edge = Color::new(0.12, 0.13, 0.14, 1.);
    box_part(
        mesh,
        vec3(0., 0.075, -0.115),
        vec3(0.075, 0.075, 0.33),
        slide,
    );
    box_part(mesh, vec3(0., 0.03, -0.10), vec3(0.068, 0.032, 0.29), black);
    box_part(mesh, vec3(0., -0.072, 0.005), vec3(0.07, 0.18, 0.08), black);
    box_part(mesh, vec3(0., -0.17, 0.006), vec3(0.079, 0.017, 0.09), edge);
    // Barrel opening, front/rear sights, trigger and open trigger guard.
    box_part(
        mesh,
        vec3(0., 0.074, -0.282),
        vec3(0.041, 0.041, 0.006),
        black,
    );
    box_part(
        mesh,
        vec3(0., 0.124, -0.245),
        vec3(0.013, 0.024, 0.018),
        black,
    );
    for x in [-0.023, 0.023] {
        box_part(mesh, vec3(x, 0.12, 0.017), vec3(0.012, 0.018, 0.022), black);
    }
    box_part(
        mesh,
        vec3(0., -0.068, -0.106),
        vec3(0.04, 0.014, 0.13),
        black,
    );
    box_part(
        mesh,
        vec3(0., -0.031, -0.166),
        vec3(0.04, 0.06, 0.014),
        black,
    );
    box_part(
        mesh,
        vec3(0., -0.024, -0.074),
        vec3(0.013, 0.045, 0.016),
        edge,
    );
    for z in [-0.018, 0., 0.018] {
        for x in [-0.039, 0.039] {
            box_part(mesh, vec3(x, 0.078, z), vec3(0.004, 0.05, 0.006), edge);
        }
    }
    box_part(
        mesh,
        vec3(0.039, 0.08, -0.096),
        vec3(0.003, 0.025, 0.05),
        black,
    );
}

fn tool_target(width: u32, height: u32) -> RenderTarget {
    render_target_ex(
        width,
        height,
        RenderTargetParams {
            depth: true,
            ..Default::default()
        },
    )
}

// A continuous forged profile rather than a crossbar with two rectangular prongs.
// Both camera perspectives reuse this mesh. Construction happens only at startup.
fn forged_wrench(mesh: &mut Mesh) {
    let steel = Color::new(0.72, 0.76, 0.79, 1.);
    let bright = Color::new(0.91, 0.94, 0.96, 1.);
    let edge = Color::new(0.40, 0.46, 0.51, 1.);
    // Paired outer/inner contours run from one jaw tip around the heel to the other.
    // The parallel inner jaw faces grip a nut; the angled mouth stays genuinely open.
    let outline = [
        ((-0.083, 0.119), (-0.054, 0.092)),
        ((-0.126, 0.077), (-0.054, 0.054)),
        ((-0.143, 0.023), (-0.054, 0.013)),
        ((-0.131, -0.034), (-0.043, -0.016)),
        ((-0.094, -0.076), (-0.024, -0.032)),
        ((-0.039, -0.099), (0., -0.037)),
        ((0.024, -0.101), (0.024, -0.032)),
        ((0.085, -0.078), (0.043, -0.016)),
        ((0.125, -0.035), (0.054, 0.013)),
        ((0.135, 0.023), (0.054, 0.054)),
        ((0.112, 0.082), (0.054, 0.092)),
    ];
    let rotate = Mat2::from_angle(-15_f32.to_radians());
    let profile: Vec<_> = outline
        .iter()
        .map(|&((ox, oy), (ix, iy))| {
            let outer = rotate * vec2(ox, oy) + vec2(0., 0.46);
            let inner = rotate * vec2(ix, iy) + vec2(0., 0.46);
            let bevel = (inner - outer).normalize() * 0.006;
            [outer, outer + bevel, inner - bevel, inner]
        })
        .collect();
    for pair in profile.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        for sign in [-1., 1.] {
            // Broad flat faces and narrow bright chamfers along both contours.
            for (lo, hi, za, zb, color) in [
                (0, 1, 0.014, 0.023, bright),
                (1, 2, 0.023, 0.023, steel),
                (2, 3, 0.023, 0.014, edge),
            ] {
                metal_quad(
                    mesh,
                    [
                        a[lo].extend(za * sign),
                        b[lo].extend(za * sign),
                        b[hi].extend(zb * sign),
                        a[hi].extend(zb * sign),
                    ],
                    color,
                );
            }
        }
        for i in [0, 3] {
            metal_quad(
                mesh,
                [
                    a[i].extend(-0.014),
                    b[i].extend(-0.014),
                    b[i].extend(0.014),
                    a[i].extend(0.014),
                ],
                edge,
            );
        }
    }
    for end in [&profile[0], &profile[profile.len() - 1]] {
        metal_quad(
            mesh,
            [
                end[1].extend(-0.023),
                end[2].extend(-0.023),
                end[2].extend(0.023),
                end[1].extend(0.023),
            ],
            steel,
        );
        for (rim, face) in [(0, 1), (3, 2)] {
            metal_quad(
                mesh,
                [
                    end[rim].extend(-0.014),
                    end[face].extend(-0.023),
                    end[face].extend(0.023),
                    end[rim].extend(0.014),
                ],
                steel,
            );
        }
    }
    // Tapered, flattened shank with chamfered edges. The shoulder enters the head heel.
    let stations = [
        (-0.14, 0.032),
        (-0.115, 0.045),
        (0.02, 0.037),
        (0.27, 0.026),
        (0.35, 0.041),
        (0.38, 0.05),
    ];
    for pair in stations.windows(2) {
        let [(ya, wa), (yb, wb)] = [pair[0], pair[1]];
        for sign in [-1., 1.] {
            metal_quad(
                mesh,
                [
                    vec3(-wa + 0.007, ya, 0.019 * sign),
                    vec3(wa - 0.007, ya, 0.019 * sign),
                    vec3(wb - 0.007, yb, 0.019 * sign),
                    vec3(-wb + 0.007, yb, 0.019 * sign),
                ],
                steel,
            );
            for side in [-1., 1.] {
                metal_quad(
                    mesh,
                    [
                        vec3((wa - 0.007) * side, ya, 0.019 * sign),
                        vec3(wa * side, ya, 0.009 * sign),
                        vec3(wb * side, yb, 0.009 * sign),
                        vec3((wb - 0.007) * side, yb, 0.019 * sign),
                    ],
                    bright,
                );
            }
        }
        for side in [-1., 1.] {
            metal_quad(
                mesh,
                [
                    vec3(wa * side, ya, -0.009),
                    vec3(wb * side, yb, -0.009),
                    vec3(wb * side, yb, 0.009),
                    vec3(wa * side, ya, 0.009),
                ],
                edge,
            );
        }
    }
    for &(y, width) in [&stations[0], &stations[stations.len() - 1]] {
        metal_quad(
            mesh,
            [
                vec3(-width + 0.007, y, -0.019),
                vec3(width - 0.007, y, -0.019),
                vec3(width - 0.007, y, 0.019),
                vec3(-width + 0.007, y, 0.019),
            ],
            edge,
        );
        for side in [-1., 1.] {
            metal_quad(
                mesh,
                [
                    vec3(width * side, y, -0.009),
                    vec3((width - 0.007) * side, y, -0.019),
                    vec3((width - 0.007) * side, y, 0.019),
                    vec3(width * side, y, 0.009),
                ],
                edge,
            );
        }
    }
    // Shallow forged recess on the handle, distinct from a rubber baton grip.
    box_part(mesh, vec3(0., 0.17, -0.020), vec3(0.019, 0.21, 0.002), edge);
}

fn metal_quad(mesh: &mut Mesh, points: [Vec3; 4], color: Color) {
    let base = mesh.vertices.len() as u16;
    for point in points {
        mesh.vertices
            .push(Vertex::new(point.x, point.y, point.z, 0., 0., color));
    }
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(super) use crate::viewer::game_visuals::box_part;

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

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
}
