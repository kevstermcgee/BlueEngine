//! BlueEngine kit geometry and live canvas shaders. The current scene is the only cached scene.
use macroquad::{
    miniquad::{Comparison, UniformDesc, UniformType},
    prelude::*,
};
use reverie::{
    art_position,
    collection::{mix, PALETTES},
    door_position, Sim,
};
use std::f32::consts::{PI, TAU};
use vesper3d::viewer::kit::{Batch, Look, Materials, Template, Tint, View};
const GOLD: [f32; 3] = [0.69, 0.46, 0.22];
const IVORY: [f32; 3] = [0.67, 0.61, 0.49];
fn v(p: vesper3d::math::V) -> Vec3 {
    vec3(p.0, p.1, p.2)
}
fn scale(c: [f32; 3], s: f32) -> [f32; 3] {
    [c[0] * s, c[1] * s, c[2] * s]
}
fn rand(seed: u64, n: u64) -> f32 {
    (mix(seed ^ n) % 100000) as f32 / 100000.
}

pub struct Canvas {
    mesh: Mesh,
    pub seed: u64,
    pub mood: usize,
    pub center: Vec3,
    pub width: f32,
    pub height: f32,
    pub yaw: f32,
    pub motif: usize,
}
impl Canvas {
    fn new(center: Vec3, width: f32, height: f32, yaw: f32, seed: u64, mood: usize, motif: usize) -> Self {
        let rotation = Mat4::from_rotation_y(yaw);
        let corners = [
            vec3(-width / 2., -height / 2., 0.),
            vec3(width / 2., -height / 2., 0.),
            vec3(width / 2., height / 2., 0.),
            vec3(-width / 2., height / 2., 0.),
        ];
        let uv = [vec2(0., 0.), vec2(1., 0.), vec2(1., 1.), vec2(0., 1.)];
        let verts = corners
            .iter()
            .zip(uv)
            .map(|(p, u)| Vertex::new2(center + rotation.transform_point3(*p), u, WHITE))
            .collect();
        Self {
            mesh: Mesh { vertices: verts, indices: vec![0, 1, 2, 0, 2, 3], texture: None },
            seed,
            mood,
            center,
            width,
            height,
            yaw,
            motif,
        }
    }
    fn draw(&self, material: &Material, time: f32, eye: Vec3) {
        let (a, b) = PALETTES[self.mood];
        material.set_uniform("Time", time);
        material.set_uniform("Seed", (self.seed % 16381) as f32);
        material.set_uniform("Mood", self.mood as f32);
        material.set_uniform("Variant", self.motif as f32);
        material.set_uniform("A", Vec3::from(a));
        material.set_uniform("B", Vec3::from(b));
        material.set_uniform("Ratio", self.width / self.height);
        material.set_uniform("Near", (1. - eye.distance(self.center) / 5.).clamp(0., 1.));
        gl_use_material(material);
        draw_mesh(&self.mesh);
    }
}
struct Living {
    mesh: Template,
    at: Vec3,
    style: usize,
    phase: f32,
    size: f32,
}
pub struct Scene {
    pub stamp: (u64, usize),
    static_meshes: Vec<Mesh>,
    sky: Vec<Mesh>,
    canvases: Vec<Canvas>,
    living: Vec<Living>,
    mote: Template,
    key: Template,
    doors: Template,
}
fn append_mesh(out: &mut Vec<Mesh>, t: Template) {
    out.extend(t.to_meshes());
}
fn pedestal(t: &mut Template, at: Vec3) {
    t.box_(at + vec3(0., 0.43, 0.), vec3(0.65, 0.43, 0.65), [0.19, 0.18, 0.17], 0.);
    t.box_(at + vec3(0., 0.89, 0.), vec3(0.7, 0.03, 0.7), IVORY, 0.);
    t.box_(at + vec3(0., 0.04, 0.), vec3(0.7, 0.04, 0.7), GOLD, 0.);
    t.box_(at + vec3(0., 0.7, 0.658), vec3(0.35, 0.13, 0.008), [0.55, 0.45, 0.29], 0.3);
}
fn frame(canvas: &Canvas, style: usize, out: &mut Vec<Mesh>) {
    let mut t = Template::new();
    let (w, h) = (canvas.width / 2., canvas.height / 2.);
    let col = match style {
        1 => [0.13, 0.14, 0.13],
        2 => [0.67, 0.54, 0.4],
        3 => [0.46, 0.57, 0.56],
        _ => GOLD,
    };
    let thick = if style == 1 { 0.05 } else { 0.12 };
    for layer in 0..if style == 2 { 3 } else { 2 } {
        let d = layer as f32 * 0.1;
        t.box_(vec3(-w - d, 0., -d * 0.3), vec3(thick, h + d, 0.06), scale(col, 1. - d), 0.15);
        t.box_(vec3(w + d, 0., -d * 0.3), vec3(thick, h + d, 0.06), scale(col, 1. - d), 0.15);
        t.box_(vec3(0., -h - d, -d * 0.3), vec3(w + d + thick, thick, 0.06), col, 0.1);
        t.box_(vec3(0., h + d, -d * 0.3), vec3(w + d + thick, thick, 0.06), col, 0.1);
    }
    if style == 0 || style == 4 {
        for side in [-1., 1.] {
            for i in 0..18 {
                let p = i as f32 / 17.;
                t.ball(
                    vec3(side * (w + 0.15), -h + p * 2. * h, 0.06),
                    vec3(0.065, 0.09, 0.05),
                    [0.85, 0.65, 0.34],
                    0.2,
                    6,
                    4,
                );
            }
        }
        for x in [-w, w] {
            for y in [-h, h] {
                t.ball(vec3(x, y, 0.06), vec3(0.19, 0.19, 0.08), GOLD, 0.2, 10, 5);
                t.ball(vec3(x, y, 0.14), Vec3::splat(0.07), [0.91, 0.77, 0.46], 0.4, 8, 4);
            }
        }
        if style == 4 {
            for i in 0..15 {
                let a = i as f32 / 14. * PI;
                t.ball(vec3(a.cos() * w * 0.65, h + a.sin() * 0.5, 0.), Vec3::splat(0.055), GOLD, 0.25, 6, 4);
            }
        }
    }
    // A brass plaque belongs to every work, with its readable full label supplied by Q.
    t.box_(vec3(0., -h - 0.38, -0.035), vec3(w.min(0.9), 0.12, 0.025), [0.51, 0.43, 0.29], 0.2);
    append_mesh(out, t.transformed(Mat4::from_translation(canvas.center) * Mat4::from_rotation_y(canvas.yaw)));
}
fn sculpture(seed: u64, style: usize, mood: usize) -> Template {
    let (a, b) = PALETTES[mood];
    let mut t = Template::new();
    let phase = rand(seed, 7) * TAU;
    match style % 6 {
        0 => {
            // A continuously curled, physically modelled bronze knot.
            let points: Vec<_> = (0..=100)
                .map(|i| {
                    let u = i as f32 / 100. * TAU;
                    vec3((u * 2.).sin() * 0.67, (u * 3. + phase).cos() * 0.32 + 0.15, u.cos() * 0.63)
                })
                .collect();
            for w in points.windows(2) {
                t.rod(w[0], w[1], 0.095, 0.095, b, 0.15, 8);
            }
        }
        1 => {
            // A shattered heart made from suspended droplets.
            for i in 0..32 {
                let u = i as f32 / 32. * TAU;
                let x = u.sin().powi(3) * 0.65;
                let y = (13. * u.cos() - 5. * (2. * u).cos() - 2. * (3. * u).cos() - (4. * u).cos()) / 21.;
                t.ball(
                    vec3(x, y * 0.65 + 0.25, (u * 3. + phase).sin() * 0.11),
                    Vec3::splat(0.07 + rand(seed, i) * 0.07),
                    b,
                    0.55,
                    8,
                    5,
                );
            }
        }
        2 => {
            // A prism blooming into a mechanical flower.
            for i in 0..18 {
                let u = i as f32 / 18. * TAU;
                let p = vec3(u.cos() * 0.62, (u * 3. + phase).sin() * 0.25, u.sin() * 0.62);
                t.rod(Vec3::ZERO, p, 0.03, 0.07, a, 0.6, 5);
                t.ball(p, vec3(0.15, 0.26, 0.15), b, 0.25, 8, 5);
            }
            t.ball(Vec3::ZERO, Vec3::splat(0.25), b, 0.7, 14, 8);
        }
        3 => {
            // A staircase that circles an empty centre.
            for i in 0..26 {
                let u = i as f32 * 0.44 + phase;
                let p = vec3(u.cos() * 0.5, i as f32 * 0.035 - 0.3, u.sin() * 0.5);
                let mut block = Template::new();
                block.box_(Vec3::ZERO, vec3(0.25, 0.045, 0.14), if i % 2 == 0 { a } else { b }, 0.25);
                t.append(&block.transformed(Mat4::from_translation(p) * Mat4::from_rotation_y(-u)));
            }
        }
        4 => {
            // Orbits with no central planet.
            for ring in 0..3 {
                for i in 0..48 {
                    let u = i as f32 / 48. * TAU;
                    let p = vec3(u.cos() * 0.62, u.sin() * 0.62, 0.);
                    let q = vec3((u + TAU / 48.).cos() * 0.62, (u + TAU / 48.).sin() * 0.62, 0.);
                    let r = Mat4::from_rotation_y(ring as f32 * PI / 3.);
                    t.rod(r.transform_point3(p), r.transform_point3(q), 0.035, 0.035, b, 0.4, 6);
                }
            }
        }
        _ => {
            // A listening forest of light.
            for i in 0..21 {
                let u = i as f32 * 2.399;
                let r = (i as f32 / 21.).sqrt() * 0.65;
                let h = 0.5 + rand(seed, i) * 0.8;
                let p = vec3(u.cos() * r, -0.5, u.sin() * r);
                t.cylinder(p, 0.024, h, a, 0.6, 6);
                t.ball(p + vec3(0., h, 0.), Vec3::splat(0.055), b, 1., 8, 4);
            }
        }
    }
    t
}
fn door_geometry(t: &mut Template, at: Vec3, back: bool) {
    let z = if back { -1. } else { 1. };
    t.box_(at + vec3(0., 0., -0.08 * z), vec3(0.85, 1.65, 0.055), [0.10, 0.115, 0.11], 0.);
    for x in [-0.92, 0.92] {
        t.box_(at + vec3(x, 0., 0.), vec3(0.085, 1.75, 0.09), GOLD, 0.15);
    }
    t.box_(at + vec3(0., 1.72, 0.), vec3(1., 0.08, 0.09), GOLD, 0.15);
    for x in [-0.4, 0.4] {
        t.box_(at + vec3(x, 0., 0.03 * z), vec3(0.33, 1.45, 0.012), [0.2, 0.22, 0.19], 0.1);
        for y in [-0.95, 0., 0.95] {
            t.box_(at + vec3(x, y, 0.06 * z), vec3(0.32, 0.01, 0.015), GOLD, 0.3);
        }
    }
    t.ball(at + vec3(0.12, -0.1, 0.10 * z), Vec3::splat(0.055), [0.91, 0.72, 0.38], 0.7, 10, 6);
    for i in 0..25 {
        let a = i as f32 / 24. * PI;
        t.ball(at + vec3(a.cos() * 0.84, 1.65 + a.sin() * 0.54, 0.), Vec3::splat(0.035), GOLD, 0.5, 5, 3);
    }
}
impl Scene {
    pub fn build(sim: &Sim) -> Self {
        let seed = if sim.depth() == 0 { sim.seed() } else { sim.world_seed() };
        let mood = if sim.depth() == 0 { sim.emotion() } else { sim.world_emotion() };
        let (a, b) = PALETTES[mood];
        let mut statics = vec![];
        let mut canvases = vec![];
        let mut living = vec![];
        let mut sky = Template::new();
        sky.sky_dome(
            95.,
            |e| {
                let v = e.clamp(0., 1.);
                let c = scale(a, 0.24 + 0.35 * (1. - v));
                [c[0] + 0.03, c[1] + 0.025, c[2] + 0.025]
            },
            48,
            24,
        );
        let mut t = Template::new();
        if sim.depth() == 0 {
            // Warm limestone, recessed paneling, brass inlays and a skylit vault.
            t.box_(vec3(0., -0.13, 0.), vec3(6., 0.13, 9.), [0.105, 0.12, 0.115], 0.);
            for x in -5..=5 {
                t.box_(vec3(x as f32, 0.005, 0.), vec3(0.008, 0.004, 9.), [0.32, 0.28, 0.19], 0.1);
            }
            for z in -8..=8 {
                t.box_(vec3(0., 0.005, z as f32), vec3(6., 0.004, 0.008), [0.32, 0.28, 0.19], 0.1);
            }
            for x in [-6.15, 6.15] {
                t.box_(vec3(x, 3.1, 0.), vec3(0.15, 3.1, 9.15), IVORY, 0.);
                t.box_(vec3(x * 0.972, 0.24, 0.), vec3(0.06, 0.24, 9.), [0.39, 0.35, 0.29], 0.);
                t.box_(vec3(x * 0.966, 4.9, 0.), vec3(0.08, 0.08, 9.), GOLD, 0.15);
            }
            for z in [-9.15, 9.15] {
                t.box_(vec3(0., 3.1, z), vec3(6., 3.1, 0.15), IVORY, 0.);
            }
            for z in [-7., 0., 7.] {
                for x in [-5.9, 5.9] {
                    t.box_(vec3(x, 2.6, z), vec3(0.20, 2.6, 0.29), [0.73, 0.67, 0.54], 0.);
                    t.box_(vec3(x, 0.23, z), vec3(0.30, 0.23, 0.36), [0.49, 0.43, 0.32], 0.);
                    t.box_(vec3(x, 5.08, z), vec3(0.34, 0.15, 0.41), GOLD, 0.15);
                }
            }
            // A coffered roof surrounding the bright central glass slot.
            for x in [-4.35, 4.35] {
                t.box_(vec3(x, 6.1, 0.), vec3(1.65, 0.1, 9.), [0.37, 0.34, 0.28], 0.);
            }
            for z in [-8.9, -5.9, -2.9, 0.1, 3.1, 6.1, 8.9] {
                t.box_(vec3(0., 5.85, z), vec3(6., 0.18, 0.11), [0.42, 0.35, 0.23], 0.);
                t.box_(vec3(0., 6.04, z), vec3(2.8, 0.01, 0.05), GOLD, 0.5);
            }
            t.quad_facing(
                [vec3(-2.7, 6.3, -9.), vec3(2.7, 6.3, -9.), vec3(2.7, 6.3, 9.), vec3(-2.7, 6.3, 9.)],
                -Vec3::Y,
                [0.78, 0.83, 0.74],
                0.8,
            );
            // Quiet benches leave the central walkway clear.
            for x in [-4.6, 4.6] {
                t.box_(vec3(x, 0.45, 4.5), vec3(0.43, 0.11, 1.25), [0.28, 0.20, 0.12], 0.);
                for z in [3.6, 5.4] {
                    t.box_(vec3(x, 0.22, z), vec3(0.3, 0.22, 0.08), GOLD, 0.);
                }
            }
            for s in 0..3 {
                let art = sim.art(s);
                let (w, h, yaw) =
                    if s == 0 { (5.1, 3.1, 0.) } else { (3.4, 2.6, if s == 1 { PI / 2. } else { -PI / 2. }) };
                let c = Canvas::new(v(art_position(s)), w, h, yaw, art.seed, art.emotion, s);
                frame(&c, art.frame, &mut statics);
                canvases.push(c);
            }
            for s in 3..6 {
                let at = v(art_position(s));
                pedestal(&mut t, vec3(at.x, 0., at.z));
                let art = sim.art(s);
                living.push(Living {
                    mesh: sculpture(art.seed, (art.seed % 6) as usize, art.emotion),
                    at: at + vec3(0., 0.05, 0.),
                    style: s,
                    phase: rand(art.seed, 2) * TAU,
                    size: 1.,
                });
            }
            for branch in 0..2 {
                door_geometry(&mut t, v(door_position(branch)), false);
            }
            if !sim.archive.room.is_empty() {
                door_geometry(&mut t, vec3(0., 1.65, 8.65), true);
            } else {
                t.box_(vec3(0., 2.4, 8.94), vec3(2.4, 1.5, 0.02), [0.14, 0.17, 0.15], 0.);
            }
        } else {
            // The canvas becomes a boundless-feeling, walkable landscape with a safe continuous floor.
            t.box_(vec3(0., -0.15, 0.), vec3(24., 0.15, 24.), scale(a, 0.23), 0.15);
            for z in -23..24 {
                let glow = 0.05 + 0.12 * (z as f32 * 0.4).sin().abs();
                t.box_(vec3(0., 0.008, z as f32), vec3(1.5, 0.008, 0.46), scale(b, 0.40), glow);
            }
            for x in [-1.56, 1.56] {
                t.box_(vec3(x, 0.02, 0.), vec3(0.015, 0.014, 23.), b, 0.6);
            }
            // A quiet lake of luminous contour lines, interrupted by impossible islands.
            for i in 0..22 {
                let r = 3. + i as f32 * 1.2;
                t.ring(vec3(0., 0.022, -6.), r, r + 0.025, scale(b, 0.48), 0.4, 80);
            }
            append_mesh(&mut statics, t);
            t = Template::new();
            let motif = (seed % 4) as usize;
            for i in 0..30 {
                let u = rand(seed, i * 7) * TAU;
                let r = 7. + rand(seed, i * 7 + 1) * 14.;
                let p = vec3(u.cos() * r, 0., u.sin() * r - 2.);
                let h = 2. + rand(seed, i * 7 + 2) * 8.;
                if motif == 0 || mood == 2 {
                    // trees remembering stars
                    t.rod(p, p + vec3(0., h, 0.), 0.15, 0.045, scale(a, 0.65), 0.15, 8);
                    for branch in 0..5 {
                        let bpos = p + vec3(0., h * (0.4 + branch as f32 * 0.11), 0.);
                        let ang = u + branch as f32 * 2.399;
                        let end = bpos + vec3(ang.cos() * 1.6, 0.9, ang.sin() * 1.6);
                        t.rod(bpos, end, 0.065, 0.014, scale(b, 0.6), 0.35, 6);
                        t.ball(end, Vec3::splat(0.15), b, 1., 8, 5);
                    }
                } else if motif == 1 {
                    // monolithic memory architecture
                    t.box_(p + vec3(0., h / 2., 0.), vec3(0.55, h / 2., 0.38), scale(a, 0.55), 0.12);
                    t.box_(p + vec3(0., h + 0.1, 0.), vec3(0.62, 0.05, 0.43), b, 0.7);
                } else if motif == 2 {
                    // flowers bigger than a person
                    t.rod(p, p + vec3(0., h, 0.), 0.07, 0.04, scale(a, 0.7), 0.2, 7);
                    for petal in 0..6 {
                        let ang = petal as f32 * TAU / 6.;
                        t.ball(p + vec3(ang.cos() * 0.7, h, ang.sin() * 0.7), vec3(0.48, 0.12, 0.48), b, 0.35, 12, 6);
                    }
                    t.ball(p + vec3(0., h + 0.1, 0.), Vec3::splat(0.25), [0.98, 0.86, 0.55], 0.9, 12, 8);
                } else {
                    // an archipelago of floating stones
                    living.push(Living {
                        mesh: sculpture(seed ^ i, 4, mood),
                        at: p + vec3(0., h, 0.),
                        style: 4,
                        phase: u,
                        size: 1.,
                    });
                    t.cone(p, 1.2, 0.3, 0.6, scale(a, 0.45), 0.1, 12);
                }
                if t.verts.len() > 45000 {
                    append_mesh(&mut statics, t);
                    t = Template::new();
                }
            }
            let shape = if sim.depth() == 1 { sim.archive.inside[0] as usize } else { (seed % 4) as usize };
            living.push(Living {
                mesh: sculpture(seed, if shape == 2 { 2 } else { 4 }, mood),
                at: vec3(0., 7., -5.),
                style: 4,
                phase: 0.,
                size: 4.,
            });
            // A huge suspended sun and its tilted celestial rings.
            t.ball(vec3(-9., 13., -19.), Vec3::splat(3.4), b, 0.85, 36, 22);
            let mut rings = Template::new();
            for j in 0..3 {
                rings.ring(Vec3::ZERO, 4.2 + j as f32 * 0.35, 4.26 + j as f32 * 0.35, scale(a, 0.8), 0.8, 100);
            }
            t.append(&rings.transformed(Mat4::from_translation(vec3(-9., 13., -19.)) * Mat4::from_rotation_x(0.4)));
            let art = sim.inner_art();
            let c = Canvas::new(vec3(0., 2.4, -11.), 3.4, 3.0, 0., art.seed, art.emotion, (art.seed % 4) as usize);
            frame(&c, art.frame, &mut statics);
            canvases.push(c);
            for x in [-2., 2.] {
                t.cylinder(vec3(x, 0., -11.), 0.09, 4.7, GOLD, 0.6, 10);
            }
            // The white return frame always faces back into the work.
            door_geometry(&mut t, vec3(0., 1.7, 13.), true);
        }
        append_mesh(&mut statics, t);
        let mut key = Template::new();
        for i in 0..28 {
            let u = i as f32 / 28. * TAU;
            let q = (i + 1) as f32 / 28. * TAU;
            key.rod(
                vec3(u.cos() * 0.18, u.sin() * 0.18, 0.),
                vec3(q.cos() * 0.18, q.sin() * 0.18, 0.),
                0.035,
                0.035,
                [1., 0.75, 0.24],
                1.,
                6,
            );
        }
        key.box_(vec3(0., -0.33, 0.), vec3(0.038, 0.25, 0.038), [1., 0.75, 0.24], 1.);
        for y in [-0.45, -0.56] {
            key.box_(vec3(0.08, y, 0.), vec3(0.09, 0.035, 0.04), [1., 0.8, 0.35], 1.);
        }
        let mut mote = Template::new();
        mote.ball(Vec3::ZERO, Vec3::splat(0.014), b, 0.9, 5, 3);
        let mut doors = Template::new();
        doors.ball(Vec3::ZERO, Vec3::splat(0.09), [1., 0.83, 0.4], 1., 10, 5);
        Self {
            stamp: (seed, sim.depth()),
            static_meshes: statics,
            sky: sky.to_meshes(),
            canvases,
            living,
            mote,
            key,
            doors,
        }
    }
    pub fn draw(
        &self,
        sim: &Sim,
        m: &Materials,
        canvas_material: &Material,
        view: &View,
        time: f32,
        world: &mut Batch,
    ) {
        let mood = if sim.depth() == 0 { sim.emotion() } else { sim.world_emotion() };
        let (a, b) = PALETTES[mood];
        let mut look = Look::daylight();
        if sim.depth() == 0 {
            look.ambient_sky = [0.56, 0.54, 0.46];
            look.ambient_ground = [0.27, 0.25, 0.21];
            look.key_color = [0.79, 0.76, 0.61];
            look.fog_color = [0.18, 0.19, 0.16];
            look.fog_density = 0.012;
            look.key_direction = [-0.3, 0.85, 0.1];
            look.rim_strength = 0.15;
        } else {
            look.ambient_sky = scale(a, 0.6);
            look.ambient_ground = scale(a, 0.22);
            look.key_color = scale(b, 0.8);
            look.fog_color = scale(a, 0.24);
            look.fog_density = 0.017;
            look.rim_color = b;
            look.rim_strength = 0.25;
        }
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&m.sky);
        for mesh in &self.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera_checked(0.07, 110.));
        m.set_scene(&look, view.eye, time, 0.6 + 0.2 * (time * 0.45).sin());
        m.draw_static(&self.static_meshes);
        world.clear();
        for l in &self.living {
            let energy = if sim.depth() == 0 && sim.archive.awakened.contains(&sim.art_id(l.style)) { 1.8 } else { 1. };
            let bob = (time * 0.6 + l.phase).sin() * 0.12 * energy;
            let breathing = 1. + (time * 0.75 + l.phase).sin() * 0.035 * energy;
            world.add(
                &l.mesh,
                Mat4::from_translation(l.at + vec3(0., bob, 0.))
                    * Mat4::from_rotation_y(time * 0.09 * energy + l.phase)
                    * Mat4::from_scale(Vec3::splat(breathing * l.size)),
                Tint::NONE,
            );
        }
        let seed = if sim.depth() == 0 { sim.seed() } else { sim.world_seed() };
        for i in 0..if sim.depth() == 0 { 45 } else { 160 } {
            let x = (rand(seed, i * 3) * 2. - 1.) * if sim.depth() == 0 { 5.5 } else { 22. };
            let z = (rand(seed, i * 3 + 1) * 2. - 1.) * if sim.depth() == 0 { 8.5 } else { 22. };
            let y = 0.6 + (rand(seed, i * 3 + 2) * 4. + time * 0.06).rem_euclid(4.5);
            world.add(
                &self.mote,
                Mat4::from_translation(vec3(x + (time * 0.3 + i as f32).sin() * 0.12, y, z)),
                Tint::NONE,
            );
        }
        if sim.key_visible() {
            let at = v(sim.key_position()) + vec3(0., (time * 1.2).sin() * 0.12, 0.);
            world.add(&self.key, Mat4::from_translation(at) * Mat4::from_rotation_y(time * 0.8), Tint::NONE);
            for i in 0..10 {
                let u = time * 0.4 + i as f32 * TAU / 10.;
                world.add(
                    &self.mote,
                    Mat4::from_translation(at + vec3(u.cos() * 0.5, (u * 3.).sin() * 0.2, u.sin() * 0.5))
                        * Mat4::from_scale(Vec3::splat(2.5)),
                    Tint::NONE,
                );
            }
        }
        if sim.depth() == 0 && sim.has_key() {
            for i in 0..2 {
                world.add(&self.doors, Mat4::from_translation(v(door_position(i)) + vec3(0., 1.5, 0.15)), Tint::NONE);
            }
        }
        gl_use_material(&m.world);
        world.draw();
        for c in &self.canvases {
            c.draw(canvas_material, time, view.eye);
        }
        gl_use_default_material();
        set_default_camera();
    }
}

pub fn canvas_material() -> Material {
    load_material(
        ShaderSource::Glsl { vertex: CANVAS_VERTEX, fragment: CANVAS_FRAGMENT },
        MaterialParams {
            pipeline_params: macroquad::miniquad::PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: true,
                ..Default::default()
            },
            uniforms: vec![
                UniformDesc::new("Time", UniformType::Float1),
                UniformDesc::new("Seed", UniformType::Float1),
                UniformDesc::new("Mood", UniformType::Float1),
                UniformDesc::new("Variant", UniformType::Float1),
                UniformDesc::new("A", UniformType::Float3),
                UniformDesc::new("B", UniformType::Float3),
                UniformDesc::new("Ratio", UniformType::Float1),
                UniformDesc::new("Near", UniformType::Float1),
            ],
            ..Default::default()
        },
    )
    .expect("living canvas shader")
}
const CANVAS_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
uniform mat4 Model;
uniform mat4 Projection;
varying highp vec2 uv;
void main(){gl_Position=Projection*Model*vec4(position,1.0);uv=texcoord;}
"#;
const CANVAS_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform float Time,Seed,Mood,Ratio,Near,Variant;
uniform vec3 A,B;
float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7))+Seed*0.13)*43758.5453);}
float noise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.0-2.0*f);return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+1.0),f.x),f.y);}
float fbm(vec2 p){float n=0.0,a=0.5;for(int i=0;i<4;i++){n+=noise(p)*a;p=p*2.03+3.4;a*=0.5;}return n;}
float ring(vec2 p,float r,float w){return 1.0-smoothstep(w,w*1.8,abs(length(p)-r));}
float sphere(vec3 p,float r){return length(p)-r;}
float torus(vec3 p,vec2 r){return length(vec2(length(p.xz)-r.x,p.y))-r.y;}
mat2 rot(float a){float s=sin(a),c=cos(a);return mat2(c,-s,s,c);}
float object(vec3 p){float variant=Variant;p.xz*=rot(Time*0.065+Seed*0.01);
 if(variant<1.0){p.yz*=rot(0.5);return min(torus(p,vec2(1.0,0.11)),sphere(p-vec3(0,0.65,0),0.35));}
 if(variant<2.0){vec3 q=p;q.xy*=rot(0.5*sin(Time*0.2));return min(torus(q.xzy,vec2(0.85,0.09)),torus(p,vec2(1.2,0.07)));}
 if(variant<3.0){float d=20.0;for(int i=0;i<5;i++){float j=float(i);vec3 q=p-vec3(sin(j*2.4)*0.7,j*0.4-0.8,cos(j*2.4)*0.7);d=min(d,sphere(q,0.28+0.10*sin(Time*0.3+j)));}return d;}
 vec3 q=p;q.xy*=rot(p.z*0.5+Time*0.12);return torus(q.yzx,vec2(0.9,0.13));}
void main(){float t=Time*0.13;vec2 p=(uv-0.5)*vec2(Ratio,1.0);p+=vec2(sin(t)*0.01,cos(t*0.7)*0.015)*Near;
 float grain=hash(floor(uv*1400.0))*0.018;vec3 dark=A*0.13;vec3 col=mix(dark,A*0.7,smoothstep(-0.45,0.5,p.y));
 float clouds=fbm(p*3.5+vec2(t*0.1,Seed*0.03));col+=mix(A*0.15,B*0.19,clouds)*pow(clouds,2.0);
 float sunX=-0.35+0.18*sin(Seed);vec2 sun=p-vec2(sunX,0.16);float halo=exp(-length(sun)*7.0);
 col+=B*halo*0.32;float disk=1.0-smoothstep(0.12,0.125,length(sun));
 float eclipse=1.0-smoothstep(0.105,0.11,length(sun-vec2(0.042,0.015)));col=mix(col,B*0.9,disk*(1.0-eclipse));
 for(int i=0;i<5;i++){float k=float(i);float h=-0.12-k*0.055+(fbm(vec2(p.x*3.0+k*13.0,Seed+k*2.0))-0.5)*(0.2+k*0.05);
 float mountain=1.0-smoothstep(h,h+0.005,p.y);col=mix(col,mix(A*0.16,A*0.45,k/5.0),mountain);}
 if(p.y<-0.21){float ripple=sin(p.x*21.0+t*2.0+sin(p.y*110.0+t))*0.5+0.5;float rr=sin(p.y*190.0-t*2.0+fbm(p*7.0)*8.0)*0.5+0.5;
 col=mix(A*0.15,A*0.42,rr*0.25)+B*pow(ripple,12.0)*exp(-abs(p.x-sunX)*5.0)*0.15;}
 // A living sculptural presence floats above the landscape. It changes with every canvas seed.
 vec3 ro=vec3(0.0,0.15,4.0);vec3 rd=normalize(vec3(p.x*1.8,(p.y+0.01)*1.8,-2.8));float travel=0.0;float hit=0.0;
 for(int i=0;i<38;i++){vec3 pos=ro+rd*travel;float d=object(pos);if(d<0.006){hit=1.0;break;}travel+=d*0.85;if(travel>8.0)break;}
 if(hit>0.5){vec3 pos=ro+rd*travel;vec2 e=vec2(0.007,0.0);vec3 n=normalize(vec3(object(pos+e.xyy)-object(pos-e.xyy),object(pos+e.yxy)-object(pos-e.yxy),object(pos+e.yyx)-object(pos-e.yyx)));
 float light=max(0.0,dot(n,normalize(vec3(-1.0,2.0,3.0))));float rim=pow(1.0-max(0.0,dot(n,-rd)),2.0);
 col=mix(A*0.3,B,light*0.8)+B*rim*0.4;}
 // Stars, restless threads and the suggestion of another frame hidden inside this frame.
 vec2 cells=p*70.0;vec2 g=fract(cells)-0.5;float star=step(0.982,hash(floor(cells)))*exp(-length(g)*25.0)*(0.6+0.4*sin(Time+hash(floor(cells))*30.0));
 col+=B*star*step(-0.15,p.y);
 float thread=exp(-abs(p.y-0.30-0.045*sin(p.x*5.0+t)-0.06*sin(p.x*2.0-t))*110.0);col+=mix(A,B,0.4)*thread*0.25;
 float vign=pow(max(0.0,1.0-dot(p/vec2(Ratio,1.0),p/vec2(Ratio,1.0))*1.7),0.6);
 col=col*(0.58+0.42*vign)+grain;
 // Emotions subtly change texture and light rather than placing words on the canvas.
 if(Mood>3.5&&Mood<5.5){col+=vec3(0.09,0.015,0.0)*sin(p.y*34.0+fbm(p*5.0)*7.0+t);}
 gl_FragColor=vec4(pow(max(col,vec3(0.0)),vec3(0.85)),1.0);}
"#;
