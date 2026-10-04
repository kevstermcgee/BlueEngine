//! Cached plants, a smiling rounded boy, storybook sky and engine cast shadows. Reads Sim only.
use leo::{Meadow, Sim, CHUNK_SIZE};
use macroquad::prelude::*;
use std::collections::BTreeMap;
use vesper3d::{
    math::V,
    viewer::{
        devkit::{
            procedural::{ChunkId, WorldPoint},
            Rng,
        },
        kit::{Batch, Look, Materials, Shadows, Template, Tint, View},
    },
};

pub fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
pub fn daylight(phase: f32) -> f32 {
    smooth((-(phase * std::f32::consts::TAU).cos() + 0.22) / 0.44)
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub fn look(phase: f32) -> Look {
    let day = daylight(phase);
    let elevation = -(phase * std::f32::consts::TAU).cos();
    let warm = (1. - elevation.abs() / 0.3).clamp(0., 1.);
    let sun = [(phase * std::f32::consts::TAU).sin(), elevation, 0.15];
    let light = if elevation >= 0. { sun } else { sun.map(|v| -v) };
    // The engine shadows one directional light. Fade its contribution at the horizon
    // so switching between opposite sun/moon directions cannot snap visible shadows.
    let key = mix([0.32, 0.38, 0.65], mix([0.95, 0.9, 0.69], [1., 0.52, 0.30], warm), day)
        .map(|c| c * smooth(elevation.abs() / 0.20));
    Look {
        ambient_sky: mix([0.20, 0.23, 0.36], [0.40, 0.48, 0.56], day),
        ambient_ground: mix([0.10, 0.12, 0.18], [0.24, 0.25, 0.18], day),
        key_direction: [light[0], light[1].max(0.15), light[2]],
        key_color: key,
        rim_color: mix([0.48, 0.55, 0.87], [1., 0.79, 0.47], day),
        rim_strength: 0.22,
        fog_color: mix([0.07, 0.085, 0.15], mix([0.64, 0.76, 0.73], [0.76, 0.47, 0.39], warm), day),
        fog_density: 0.026,
        exposure: 1.,
    }
}
struct ChunkArt {
    trees: Vec<Template>,
    flowers: Vec<Template>,
}
pub struct Scene {
    plants: Vec<Template>,
    chunks: BTreeMap<ChunkId, ChunkArt>,
    seed: Option<u64>,
    dome: Template,
    stars: Vec<Template>,
    body: Template,
    arm: Template,
    leg: Template,
    world: Batch,
    actors: Batch,
    sky: Batch,
    glow: Batch,
}
fn plant(kind: usize) -> Template {
    let mut t = Template::new();
    let green = [0.25, 0.48, 0.19];
    if kind < 4 {
        let bark = if kind == 1 { [0.83, 0.82, 0.72] } else { [0.30, 0.19, 0.12] };
        // The visible trunk is exactly the simulation's blocking box. Canopies/branches are cosmetic.
        t.box_(vec3(0., 1.5, 0.), vec3(0.22, 1.5, 0.22), bark, 0.);
        if kind == 1 {
            for i in 0..7 {
                t.box_(vec3(0., 0.3 + i as f32 * 0.39, -0.225), vec3(0.16, 0.035, 0.008), [0.22, 0.23, 0.21], 0.);
            }
        }
        if kind == 2 {
            for i in 0..4 {
                let y = 2. + i as f32 * 0.9;
                t.cone(vec3(0., y, 0.), 1.8 - i as f32 * 0.32, 0., 2.3, [0.18, 0.36 + i as f32 * 0.02, 0.24], 0., 9);
            }
        } else {
            let mut rng = Rng::new(kind as u64 + 40);
            let tall = if kind == 1 { 1.5 } else { 1. };
            for i in 0..7 {
                let a = i as f32 * 2.399;
                let end = vec3(a.cos() * 1.5, 3.2 + rng.range(0., 1.7) * tall, a.sin() * 1.5);
                t.rod(vec3(0., 2., 0.), end, 0.12, 0.05, bark, 0., 6);
                let color = if kind == 3 {
                    [0.31, 0.49, 0.22]
                } else {
                    [0.25 + rng.range(0., 0.13), 0.43 + rng.range(0., 0.14), 0.18]
                };
                t.ball(end, vec3(1.45, 1.25 * tall, 1.3), color, 0., 8, 5);
                if kind == 3 {
                    for b in 0..5 {
                        t.ball(
                            end + vec3((b as f32).cos() * 0.8, -0.75, (b as f32).sin() * 0.8),
                            Vec3::splat(0.08),
                            [0.81, 0.26, 0.13],
                            0.,
                            5,
                            3,
                        );
                    }
                }
            }
        }
    } else if kind == 8 {
        for i in 0..6 {
            let a = i as f32 * 2.399;
            t.quad_facing(
                [
                    vec3(-0.02, 0., 0.),
                    vec3(0.02, 0., 0.),
                    vec3(a.cos() * 0.10, 0.3 + i as f32 * 0.015, a.sin() * 0.10),
                    vec3(a.cos() * 0.10 - 0.015, 0.3, a.sin() * 0.10),
                ],
                Vec3::Z,
                green,
                0.,
            );
        }
    } else {
        for i in 0..4 {
            let a = i as f32 * 2.399;
            let root = vec3(a.cos() * 0.17, 0., a.sin() * 0.17);
            let height = match kind {
                4 => 0.12,
                5 => 0.35,
                6 => 0.42,
                _ => 0.58,
            };
            t.rod(root, root + Vec3::Y * height, 0.01, 0.006, green, 0., 4);
            let tip = root + Vec3::Y * height;
            if kind == 6 {
                t.ball(tip, vec3(0.055, 0.065, 0.055), [0.74, 0.29, 0.47], 0., 6, 4);
                for leaf in 0..3 {
                    let a = leaf as f32 * std::f32::consts::TAU / 3.;
                    t.ball(
                        root + vec3(a.cos() * 0.07, height * 0.5, a.sin() * 0.07),
                        vec3(0.055, 0.008, 0.035),
                        green,
                        0.,
                        5,
                        3,
                    );
                }
            } else {
                let (petals, color) = match kind {
                    4 => (10, [0.94, 0.93, 0.84]),
                    5 => (5, [1., 0.76, 0.12]),
                    _ => (8, [0.34, 0.46, 0.9]),
                };
                t.disc(tip, 0.025, [0.91, 0.66, 0.11], 0., 7);
                for p in 0..petals {
                    let angle = p as f32 * std::f32::consts::TAU / petals as f32;
                    let petal = vec3(angle.cos() * 0.046, 0.004, angle.sin() * 0.046);
                    t.ball(tip + petal, vec3(0.025, 0.009, 0.025), color, 0., 4, 2);
                }
            }
        }
    }
    t
}
impl Scene {
    pub fn new() -> Self {
        let mut dome = Template::new();
        dome.sky_dome(200., |_| [0.; 3], 48, 24);
        let mut stars = Template::new();
        let mut rng = Rng::new(0x4c454f);
        for i in 0..6000 {
            // A tilted band suggests the Milky Way among the scattered brighter stars.
            let dir = if i % 2 == 0 {
                let a = rng.range(0., std::f32::consts::TAU);
                vec3(a.cos(), a.sin(), rng.range(-0.12, 0.12)).normalize()
            } else {
                vec3(rng.range(-1., 1.), rng.range(-0.25, 1.), rng.range(-1., 1.)).normalize_or_zero()
            };
            let at = dir * 185.;
            let r = dir.cross(Vec3::Y).normalize_or_zero();
            let u = r.cross(dir);
            let size = if i % 37 == 0 { 0.48 } else { rng.range(0.11, 0.32) };
            let brightness = rng.range(0.40, 0.95);
            stars.quad_facing(
                [at - r * size, at + u * size, at + r * size, at - u * size],
                -dir,
                [brightness * 0.85, brightness * 0.91, brightness],
                1.,
            );
        }
        let mut body = Template::new();
        let skin = [0.95, 0.72, 0.53];
        let shirt = [0.96, 0.72, 0.32];
        let hair = [0.35, 0.22, 0.13];
        // Oval silhouette and rounded shoulders/hips instead of a box-shaped torso and bag.
        body.ball(vec3(0., 0.78, 0.), vec3(0.185, 0.25, 0.125), shirt, 0., 16, 10);
        body.capsule(vec3(-0.085, 0.54, 0.), vec3(0.085, 0.54, 0.), 0.105, [0.30, 0.46, 0.60], 0., 12);
        body.capsule(vec3(0., 0.97, 0.), vec3(0., 1.05, 0.), 0.052, skin, 0., 10);
        body.ball(vec3(0., 1.14, 0.), vec3(0.195, 0.19, 0.17), skin, 0., 20, 14);
        for x in [-0.19, 0.19] {
            body.ball(vec3(x, 1.13, 0.), vec3(0.032, 0.047, 0.025), skin, 0., 10, 6);
        }
        // One full scalp silhouette under the fringe: no exposed temples between isolated locks.
        let mut cap = Template::new();
        cap.ball(vec3(0., 1.145, 0.010), vec3(0.207, 0.20, 0.185), hair, 0., 24, 16);
        let mut above = Vec::with_capacity(cap.verts.len());
        for vertex in &mut cap.verts {
            let forehead = smooth((-vertex.p.z + 0.035) / 0.17);
            let hairline = 1.105 + forehead * 0.098;
            above.push(vertex.p.y >= hairline);
            vertex.p.y = vertex.p.y.max(hairline);
        }
        // Remove the underside rather than closing a flat disc over the face.
        cap.idx = cap
            .idx
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|face| face.iter().any(|i| above[usize::from(*i)]))
            .flatten()
            .copied()
            .collect();
        body.append(&cap);
        for x in [-0.18, 0.18] {
            body.ball(vec3(x, 1.185, -0.005), vec3(0.037, 0.075, 0.068), hair, 0., 12, 8);
        }
        // A soft side-swept fringe overlaps the cap and falls onto the forehead.
        for i in 0..6 {
            let x = -0.145 + i as f32 * 0.05;
            body.capsule(
                vec3(x - 0.025, 1.285, -0.045),
                vec3(x + 0.030, 1.214 + i as f32 * 0.006, -0.152),
                0.038,
                [0.36 + i as f32 * 0.009, 0.225 + i as f32 * 0.006, 0.135],
                0.,
                10,
            );
        }
        for x in [-0.065, 0.065] {
            body.ball(vec3(x, 1.155, -0.159), vec3(0.022, 0.024, 0.011), [0.98, 0.95, 0.86], 0., 10, 6);
            body.ball(vec3(x, 1.155, -0.169), vec3(0.011, 0.016, 0.006), [0.19, 0.25, 0.20], 0., 8, 5);
            body.ball(vec3(x - 0.003, 1.160, -0.175), Vec3::splat(0.004), [1., 1., 0.94], 0.2, 6, 4);
            body.ball(vec3(x * 1.6, 1.10, -0.147), vec3(0.034, 0.018, 0.009), [0.94, 0.55, 0.47], 0., 10, 5);
        }
        body.ball(vec3(0., 1.116, -0.169), vec3(0.018, 0.022, 0.023), skin, 0., 10, 6);
        for i in 0..8 {
            let smile = |a: f32| vec3(a * 0.048, 1.063 + a * a * 0.018, -0.163 + a * a * 0.006);
            body.rod(
                smile(i as f32 / 4. - 1.),
                smile((i + 1) as f32 / 4. - 1.),
                0.0045,
                0.0045,
                [0.48, 0.27, 0.22],
                0.,
                6,
            );
        }
        let mut arm = Template::new();
        arm.capsule(Vec3::ZERO, vec3(0., -0.28, 0.), 0.060, shirt, 0., 12);
        arm.ball(vec3(0., -0.33, 0.), Vec3::splat(0.055), skin, 0., 12, 8);
        let mut leg = Template::new();
        leg.capsule(Vec3::ZERO, vec3(0., -0.31, 0.), 0.065, [0.30, 0.46, 0.60], 0., 12);
        leg.ball(vec3(0., -0.39, -0.035), vec3(0.075, 0.065, 0.115), [0.30, 0.22, 0.16], 0., 8, 4);
        Self {
            plants: (0..9).map(plant).collect(),
            chunks: BTreeMap::new(),
            seed: None,
            dome,
            stars: stars.split(),
            body,
            arm,
            leg,
            world: Batch::new(),
            actors: Batch::new(),
            sky: Batch::new(),
            glow: Batch::new(),
        }
    }
    fn art(&self, sim: &Sim, id: ChunkId, meadow: &Meadow) -> ChunkArt {
        let mut trees = Template::new();
        let mut flowers = Template::new();
        for z in 0..8 {
            for x in 0..8 {
                let before = trees.verts.len();
                let x = x as f32 * 4.;
                let z = z as f32 * 4.;
                trees.quad_facing(
                    [vec3(x, 0., z), vec3(x + 4., 0., z), vec3(x + 4., 0., z + 4.), vec3(x, 0., z + 4.)],
                    Vec3::Y,
                    [0.3, 0.5, 0.2],
                    0.,
                );
                for v in &mut trees.verts[before..] {
                    let p = WorldPoint::new(id, [v.p.x, v.p.z], CHUNK_SIZE).unwrap();
                    let d = vesper3d::viewer::devkit::procedural::field(sim.seed, p, CHUNK_SIZE, 2).unwrap();
                    v.c = [0.20 + d * 0.08, 0.36 + d * 0.13, 0.14 + d * 0.05];
                }
            }
        }
        for p in &meadow.plants {
            // Keep square blocking trunks axis-aligned, exactly as their authoritative colliders.
            let yaw = if p.kind < 4 { 0. } else { p.yaw };
            let m = Mat4::from_translation(vec3(p.at[0], 0., p.at[1]))
                * Mat4::from_rotation_y(yaw)
                * Mat4::from_scale(Vec3::splat(p.scale));
            let geometry = self.plants[p.kind].transformed(m);
            if p.kind < 4 {
                trees.append(&geometry);
            } else {
                flowers.append(&geometry);
            }
        }
        ChunkArt { trees: trees.split(), flowers: flowers.split() }
    }
    pub fn draw(
        &mut self,
        sim: &Sim,
        alpha: f32,
        materials: &Materials,
        shadows: &mut Shadows,
        portrait: bool,
    ) -> Result<View, String> {
        if self.seed != Some(sim.seed) {
            self.chunks.clear();
            self.seed = Some(sim.seed);
        }
        self.chunks.retain(|id, _| sim.chunks.chunks().contains_key(id));
        for (id, chunk) in sim.chunks.chunks() {
            if !self.chunks.contains_key(id) {
                let art = self.art(sim, *id, chunk);
                self.chunks.insert(*id, art);
            }
        }
        let p = sim.interpolated(alpha);
        let yaw = sim.player.yaw + if portrait { std::f32::consts::PI } else { 0. };
        let focus = V(p.0, p.1 - sim.player.profile().eye_height + 0.95, p.2);
        let (distance, height, pitch) = if portrait { (2.6, 0.45, -0.10) } else { (4.8, 1.05, -0.20) };
        let desired = focus - V(yaw.sin(), 0., -yaw.cos()) * distance + V(0., height, 0.);
        let eye = sim.camera_eye(focus, desired)?;
        let mut view = View::first_person(vec3(eye.0, eye.1, eye.2), yaw, (sim.player.pitch + pitch).clamp(-1.2, 1.2));
        view.fov = 65f32.to_radians();
        let phase = sim.time().phase;
        let day = daylight(phase);
        let mood = look(phase);
        clear_background(mood.clear_color());
        let elevation = -(phase * std::f32::consts::TAU).cos();
        let warm = (1. - elevation.abs() / 0.32).clamp(0., 1.);
        for v in &mut self.dome.verts {
            let h = (v.p.y / 200.).max(0.).sqrt();
            let night = mix([0.08, 0.075, 0.16], [0.014, 0.02, 0.07], h);
            let daylight = mix([0.76, 0.80, 0.68], [0.28, 0.49, 0.69], h);
            let twilight = mix([0.98, 0.59, 0.38], [0.26, 0.25, 0.49], h);
            v.c = mix(mix(night, daylight, day), twilight, warm * (1. - h * 0.65));
        }
        self.sky.clear();
        self.glow.clear();
        self.sky.add(&self.dome, Mat4::IDENTITY, Tint::NONE);
        for stars in &self.stars {
            self.glow.add(stars, Mat4::IDENTITY, Tint { alpha: (1. - day).powi(2), ..Tint::NONE });
        }
        let mut objects = Template::new();
        let a = phase * std::f32::consts::TAU;
        let sun = vec3(a.sin(), -a.cos(), 0.15).normalize() * 180.;
        objects.ball(sun, Vec3::splat(2.7), [1., 0.80, 0.48], 1., 16, 10);
        objects.ball(-sun, Vec3::splat(1.4), [0.80, 0.84, 0.95], 1., 16, 10);
        self.sky.add(&objects, Mat4::IDENTITY, Tint::NONE);
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        self.sky.draw();
        gl_use_material(&materials.fx_add);
        self.glow.draw();
        self.world.clear();
        self.actors.clear();
        for (id, chunk) in &self.chunks {
            let [x, z] = WorldPoint { chunk: *id, local: [0.; 2] }.relative(sim.origin, CHUNK_SIZE)?;
            let dist = (x + 16. - p.0).hypot(z + 16. - p.2);
            if dist > 115. {
                continue;
            }
            let m = Mat4::from_translation(vec3(x, 0., z));
            for t in &chunk.trees {
                self.world.add(t, m, Tint::NONE);
            }
            if dist < 48. {
                for t in &chunk.flowers {
                    self.world.add(t, m, Tint::NONE);
                }
            }
        }
        let root = Mat4::from_translation(vec3(p.0, p.1 - sim.player.profile().eye_height, p.2))
            * Mat4::from_rotation_y(-sim.player.yaw);
        let speed = (sim.player.position - sim.previous).length() / vesper3d::viewer::devkit::TICK;
        let stride = ((sim.tick as f64 * 0.23).sin() as f32) * (speed / 6.).clamp(0., 1.);
        self.actors.add(&self.body, root, Tint::NONE);
        for sign in [-1., 1.] {
            self.actors.add(
                &self.leg,
                root * Mat4::from_translation(vec3(sign * 0.105, 0.44, 0.))
                    * Mat4::from_rotation_x(sign * stride * 0.55),
                Tint::NONE,
            );
            self.actors.add(
                &self.arm,
                root * Mat4::from_translation(vec3(sign * 0.19, 0.95, 0.))
                    * Mat4::from_rotation_x(-sign * stride * 0.4),
                Tint::NONE,
            );
        }
        shadows.begin_frame(&mood, vec3(focus.0, focus.1, focus.2));
        shadows.blob(vec3(p.0, p.1 - sim.player.profile().eye_height, p.2), 0.45);
        shadows.cast(|| {
            self.world.draw();
            self.actors.draw();
        });
        set_camera(&view.camera_checked(0.08, 180.));
        materials.set_scene(&mood, view.eye, (sim.tick % 2160000) as f32 / 60., 0.);
        shadows.apply(materials);
        gl_use_material(&materials.world);
        self.world.draw();
        shadows.draw_decals(materials);
        gl_use_material(&materials.world);
        self.actors.draw();
        gl_use_default_material();
        set_default_camera();
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sun_moon_cutover_fades_without_a_lighting_jump() {
        for horizon in [0.25, 0.75] {
            let before = look(horizon - 0.0001);
            let after = look(horizon + 0.0001);
            for i in 0..3 {
                assert!(before.key_color[i] < 0.0001 && after.key_color[i] < 0.0001);
                assert!((before.ambient_sky[i] - after.ambient_sky[i]).abs() < 0.002);
            }
        }
        assert!(look(0.5).key_color[0] > 0.8);
        assert!(look(0.).key_color[2] > 0.5);
    }
    #[test]
    fn seeded_chunk_art_and_boy_fit_mesh_and_physical_scale_limits() {
        let scene = Scene::new();
        let sim = Sim::new(7);
        for z in -4..=4 {
            for x in -4..=4 {
                let id = ChunkId { x, z };
                let meadow = leo::meadow(7, id).unwrap();
                let art = scene.art(&sim, id, &meadow);
                for t in art.trees.iter().chain(&art.flowers) {
                    assert!(t.verts.len() <= 9000 && t.idx.len() <= 27000);
                    assert!(t.verts.iter().all(|v| v.p.is_finite() && v.n.is_finite()));
                    assert!(t.idx.iter().all(|i| usize::from(*i) < t.verts.len()));
                }
            }
        }
        let bounds =
            vesper3d::viewer::devkit::Bounds::of(scene.body.verts.iter().map(|v| [v.p.x, v.p.y, v.p.z])).unwrap();
        bounds.expect_longest("young boy", 0.75..=1.4).unwrap();
        for phase in [0., 0.25, 0.5, 0.75, 1.] {
            assert!((0. ..=1.).contains(&daylight(phase)));
        }
        assert_eq!(daylight(0.), daylight(1.));
    }
}
