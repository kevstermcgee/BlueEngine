//! Visual effects: particles, expanding rings, fireballs, light beams, floating popups and banners.
//!
//! The window turns simulation events into calls on [`Fx`] (`fx.sparks(...)` when something is hit);
//! the simulation never knows any of it exists. [`Fx::update`] advances everything by real seconds,
//! [`Fx::draw`] packs the world-space effects into two [`Batch`]es (one drawn additively, one with
//! normal blending) and `hud::draw_popups` / `hud::draw_banners` draw the text ones on top.
use super::batch::{Batch, Rgb, Template, Tint, Vert};
use crate::viewer::devkit::Rng;
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

/// Most live particles; further requests are dropped so an explosion loop cannot eat the frame.
pub const MAX_PARTICLES: usize = 3000;

/// A cheerful default palette for confetti.
pub const CONFETTI: [Rgb; 6] = [
    [1.0, 0.20, 0.70],
    [0.20, 0.90, 1.00],
    [1.0, 0.90, 0.20],
    [0.40, 1.00, 0.45],
    [0.72, 0.40, 1.00],
    [1.0, 0.50, 0.20],
];

/// How a particle moves and is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    /// Flutters down as a tumbling quad.
    Confetti,
    /// Additive streak along its velocity.
    Spark,
    /// Soft expanding puff.
    Dust,
}

/// One particle.
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Seconds left.
    pub life: f32,
    /// Seconds it started with.
    pub max: f32,
    pub size: f32,
    pub color: Rgb,
    /// Downward acceleration in m/s^2 (negative floats up).
    pub gravity: f32,
    /// Velocity decay per second.
    pub drag: f32,
    pub spin: f32,
    pub phase: f32,
    pub kind: ParticleKind,
}

/// An expanding, fading ring (shockwave).
#[derive(Clone, Copy, Debug)]
pub struct RingFx {
    pub pos: Vec3,
    pub normal: Vec3,
    pub r0: f32,
    pub r1: f32,
    pub life: f32,
    pub max: f32,
    pub color: Rgb,
}

/// An expanding, fading glowing ball (explosion).
#[derive(Clone, Copy, Debug)]
pub struct BallFx {
    pub pos: Vec3,
    pub r0: f32,
    pub r1: f32,
    pub life: f32,
    pub max: f32,
    pub color: Rgb,
}

/// A vertical light beam rising from `pos`.
#[derive(Clone, Copy, Debug)]
pub struct BeamFx {
    pub pos: Vec3,
    pub life: f32,
    pub max: f32,
    pub color: Rgb,
}

/// Floating world-anchored text that rises and fades (`+100`).
#[derive(Clone, Debug)]
pub struct Popup {
    pub pos: Vec3,
    pub text: String,
    pub life: f32,
    pub max: f32,
    pub color: Rgb,
    pub size: f32,
    pub rise: f32,
}

/// Big centred screen text with an optional subtitle (`WAVE 3`).
#[derive(Clone, Debug)]
pub struct Banner {
    pub text: String,
    pub sub: String,
    pub life: f32,
    pub max: f32,
    pub color: Rgb,
}

/// All live effects.
pub struct Fx {
    pub particles: Vec<Particle>,
    pub rings: Vec<RingFx>,
    pub balls: Vec<BallFx>,
    pub beams: Vec<BeamFx>,
    pub popups: Vec<Popup>,
    pub banners: Vec<Banner>,
    rng: Rng,
    ring_tpl: Template,
    ball_tpl: Template,
    beam_tpl: Template,
}

fn unit_ring() -> Template {
    let mut t = Template::new();
    // A soft band: fades in over the inner side and out over the outer side.
    t.soft_ring(Vec3::ZERO, 0.80, 0.94, [1.; 3], 0.0, [1.; 3], 1.0, 1.0, 40);
    t.soft_ring(Vec3::ZERO, 0.94, 1.0, [1.; 3], 1.0, [1.; 3], 0.0, 1.0, 40);
    t
}

fn unit_ball() -> Template {
    let mut t = Template::new();
    t.ball(Vec3::ZERO, Vec3::ONE, [1.; 3], 1.0, 16, 10);
    t
}

fn unit_beam() -> Template {
    let mut t = Template::new();
    for k in 0..2 {
        let a = k as f32 * PI * 0.5;
        let (s, c) = a.sin_cos();
        let right = vec3(c, 0., s) * 0.5;
        let (base, top) = (Vec3::ZERO, Vec3::Y);
        // Bottom bright, top transparent.
        let i0 = t.verts.len() as u16;
        for (p, a) in [
            (base - right, 0.85),
            (base + right, 0.85),
            (top + right * 0.35, 0.),
            (top - right * 0.35, 0.),
        ] {
            t.verts.push(Vert {
                p,
                n: Vec3::Z,
                c: [1.; 3],
                e: 1.,
                a,
            });
        }
        t.idx
            .extend_from_slice(&[i0, i0 + 1, i0 + 2, i0, i0 + 2, i0 + 3]);
    }
    t
}

impl Fx {
    /// Effects seeded with `seed` (so a replay looks the same).
    pub fn new(seed: u64) -> Self {
        Self {
            particles: Vec::new(),
            rings: Vec::new(),
            balls: Vec::new(),
            beams: Vec::new(),
            popups: Vec::new(),
            banners: Vec::new(),
            rng: Rng::new(seed ^ 0xF00D),
            ring_tpl: unit_ring(),
            ball_tpl: unit_ball(),
            beam_tpl: unit_beam(),
        }
    }

    /// Remove everything (a round restarted).
    pub fn clear(&mut self) {
        self.particles.clear();
        self.rings.clear();
        self.balls.clear();
        self.beams.clear();
        self.popups.clear();
        self.banners.clear();
    }

    fn rand_dir(&mut self) -> Vec3 {
        loop {
            let v = vec3(
                self.rng.range(-1., 1.),
                self.rng.range(-1., 1.),
                self.rng.range(-1., 1.),
            );
            let l = v.length();
            if l > 0.05 && l <= 1. {
                return v / l;
            }
        }
    }

    fn push(&mut self, p: Particle) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(p);
        }
    }

    /// Confetti thrown in a cone around `dir` (`spread` 0 = a line, 1 = wide).
    pub fn confetti_cone(&mut self, pos: Vec3, dir: Vec3, spread: f32, speed: f32, n: usize) {
        for _ in 0..n {
            let d = (dir + self.rand_dir() * spread).normalize_or_zero();
            let s = speed * self.rng.range(0.4, 1.1);
            let color = *self.rng.pick(&CONFETTI).unwrap_or(&CONFETTI[0]);
            let life = self.rng.range(1.2, 2.4);
            let spin = self.rng.range(-14., 14.);
            let phase = self.rng.range(0., TAU);
            let size = self.rng.range(0.07, 0.15);
            self.push(Particle {
                pos,
                vel: d * s,
                life,
                max: life,
                size,
                color,
                gravity: 6.5,
                drag: 1.6,
                spin,
                phase,
                kind: ParticleKind::Confetti,
            });
        }
    }

    /// A confetti fountain: mostly up, spraying outward.
    pub fn confetti_fountain(&mut self, pos: Vec3, n: usize, power: f32) {
        for _ in 0..n {
            let a = self.rng.range(0., TAU);
            let out = self.rng.range(0.2, 1.0);
            let up = self.rng.range(0.6, 1.4);
            let d = vec3(a.cos() * out, up, a.sin() * out);
            let color = *self.rng.pick(&CONFETTI).unwrap_or(&CONFETTI[0]);
            let life = self.rng.range(1.5, 3.0);
            let spin = self.rng.range(-14., 14.);
            let phase = self.rng.range(0., TAU);
            let size = self.rng.range(0.08, 0.18);
            let boost = self.rng.range(0.6, 1.2);
            self.push(Particle {
                pos,
                vel: d * power * boost,
                life,
                max: life,
                size,
                color,
                gravity: 7.0,
                drag: 1.3,
                spin,
                phase,
                kind: ParticleKind::Confetti,
            });
        }
    }

    /// A burst of short additive sparks in every direction.
    pub fn sparks(&mut self, pos: Vec3, n: usize, speed: f32, color: Rgb) {
        for _ in 0..n {
            let d = self.rand_dir();
            let life = self.rng.range(0.25, 0.6);
            let size = self.rng.range(0.05, 0.11);
            let boost = self.rng.range(0.3, 1.0);
            self.push(Particle {
                pos,
                vel: d * speed * boost,
                life,
                max: life,
                size,
                color,
                gravity: 9.0,
                drag: 2.0,
                spin: 0.,
                phase: 0.,
                kind: ParticleKind::Spark,
            });
        }
    }

    /// Soft puffs spreading along the ground (landings, footsteps, impacts).
    pub fn dust(&mut self, pos: Vec3, n: usize, spread: f32, color: Rgb) {
        for _ in 0..n {
            let a = self.rng.range(0., TAU);
            let s = self.rng.range(0.3, 1.0) * spread;
            let life = self.rng.range(0.4, 0.8);
            let lift = self.rng.range(0.2, 0.9);
            let size = self.rng.range(0.25, 0.5);
            self.push(Particle {
                pos: pos + vec3(a.cos() * 0.2, 0.05, a.sin() * 0.2),
                vel: vec3(a.cos() * s, lift, a.sin() * s),
                life,
                max: life,
                size,
                color,
                gravity: -0.4,
                drag: 3.0,
                spin: 0.,
                phase: 0.,
                kind: ParticleKind::Dust,
            });
        }
    }

    /// An expanding ring lying perpendicular to `normal` (a shockwave on the floor is `Vec3::Y`).
    pub fn ring(&mut self, pos: Vec3, normal: Vec3, r0: f32, r1: f32, life: f32, color: Rgb) {
        if self.rings.len() < 64 {
            self.rings.push(RingFx {
                pos,
                normal: normal.normalize_or_zero(),
                r0,
                r1,
                life,
                max: life,
                color,
            });
        }
    }

    /// A glowing ball that swells to radius `r` and fades.
    pub fn fireball(&mut self, pos: Vec3, r: f32, life: f32, color: Rgb) {
        if self.balls.len() < 24 {
            self.balls.push(BallFx {
                pos,
                r0: r * 0.3,
                r1: r,
                life,
                max: life,
                color,
            });
        }
    }

    /// A vertical beam of light.
    pub fn beam(&mut self, pos: Vec3, life: f32, color: Rgb) {
        if self.beams.len() < 48 {
            self.beams.push(BeamFx {
                pos,
                life,
                max: life,
                color,
            });
        }
    }

    /// Floating text anchored to a world position (draw with `hud::draw_popups`).
    pub fn popup(&mut self, pos: Vec3, text: impl Into<String>, color: Rgb, size: f32) {
        if self.popups.len() < 40 {
            self.popups.push(Popup {
                pos,
                text: text.into(),
                life: 1.3,
                max: 1.3,
                color,
                size,
                rise: 1.4,
            });
        }
    }

    /// Big centred text; at most three at once, oldest dropped (draw with `hud::draw_banners`).
    pub fn banner(&mut self, text: impl Into<String>, sub: impl Into<String>, color: Rgb) {
        self.banners.push(Banner {
            text: text.into(),
            sub: sub.into(),
            life: 2.2,
            max: 2.2,
            color,
        });
        if self.banners.len() > 3 {
            self.banners.remove(0);
        }
    }

    /// Advance every effect by `dt` real seconds and drop the expired.
    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.life -= dt;
            p.vel.y -= p.gravity * dt;
            p.vel *= (-p.drag * dt).exp();
            if p.kind == ParticleKind::Confetti {
                // Flutter.
                p.phase += dt * 7.;
                p.vel.x += p.phase.sin() * 2.2 * dt;
                p.vel.z += p.phase.cos() * 2.2 * dt;
                p.vel.y = p.vel.y.max(-2.4);
            }
            p.pos += p.vel * dt;
        }
        self.particles.retain(|p| p.life > 0.);
        for r in &mut self.rings {
            r.life -= dt;
        }
        self.rings.retain(|r| r.life > 0.);
        for b in &mut self.balls {
            b.life -= dt;
        }
        self.balls.retain(|b| b.life > 0.);
        for b in &mut self.beams {
            b.life -= dt;
        }
        self.beams.retain(|b| b.life > 0.);
        for p in &mut self.popups {
            p.life -= dt;
            p.pos.y += p.rise * dt;
            p.rise *= (-1.2 * dt).exp();
        }
        self.popups.retain(|p| p.life > 0.);
        for b in &mut self.banners {
            b.life -= dt;
        }
        self.banners.retain(|b| b.life > 0.);
    }

    /// Add all world-space effects to two batches: `add` is drawn with the additive material (sparks,
    /// rings, fireballs, beams), `alpha` with the alpha material (confetti, dust). `eye`, `right` and
    /// `up` come from the [`View`](super::View).
    pub fn draw(&self, add: &mut Batch, alpha: &mut Batch, eye: Vec3, right: Vec3, up: Vec3) {
        for p in &self.particles {
            let t = (p.life / p.max).clamp(0., 1.);
            match p.kind {
                ParticleKind::Confetti => {
                    let a = (t * 3.).min(1.);
                    let ang = p.phase * 0.8 + p.spin * (p.max - p.life);
                    let (s, c) = ang.sin_cos();
                    // Foreshortening as it tumbles.
                    let w = p.size * (0.35 + 0.65 * (ang * 1.7).cos().abs());
                    alpha.billboard(
                        p.pos,
                        right * c + up * s,
                        -right * s + up * c,
                        w,
                        p.size * 1.3,
                        p.color,
                        a,
                        0.55,
                    );
                }
                ParticleKind::Spark => {
                    let len = (p.vel.length() * 0.035).clamp(0.03, 0.35);
                    let dir = p.vel.normalize_or_zero();
                    add.beam(p.pos, p.pos - dir * len, eye, p.size, p.color, t, 0., 1.0);
                }
                ParticleKind::Dust => {
                    let s = p.size * (1.6 - t * 0.9);
                    alpha.billboard(p.pos, right, up, s, s, p.color, t * 0.35, 0.0);
                }
            }
        }
        for r in &self.rings {
            let t = 1. - (r.life / r.max).clamp(0., 1.);
            let ease = 1. - (1. - t) * (1. - t);
            let radius = r.r0 + (r.r1 - r.r0) * ease;
            let fade = (1. - t).powf(0.8);
            let rot = Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, r.normal));
            let m =
                Mat4::from_translation(r.pos) * rot * Mat4::from_scale(vec3(radius, 1., radius));
            add.add(
                &self.ring_tpl,
                m,
                Tint {
                    mul: r.color,
                    alpha: fade * 0.7,
                    glow: 0.9,
                    ..Tint::NONE
                },
            );
        }
        for b in &self.balls {
            let t = 1. - (b.life / b.max).clamp(0., 1.);
            let radius = b.r0 + (b.r1 - b.r0) * (1. - (1. - t) * (1. - t));
            let fade = (1. - t).powi(2) * 0.85;
            add.add(
                &self.ball_tpl,
                Mat4::from_translation(b.pos) * Mat4::from_scale(Vec3::splat(radius)),
                Tint {
                    mul: b.color,
                    alpha: fade * 0.75,
                    glow: 0.7,
                    ..Tint::NONE
                },
            );
        }
        for b in &self.beams {
            let t = (b.life / b.max).clamp(0., 1.);
            let grow = (1. - t).min(0.2) / 0.2;
            let spread = 1.0 + 0.5 * (1. - t);
            add.add(
                &self.beam_tpl,
                Mat4::from_translation(b.pos)
                    * Mat4::from_scale(vec3(spread, 8. * grow.max(0.05), spread)),
                Tint {
                    mul: b.color,
                    alpha: t.sqrt(),
                    glow: 1.4,
                    ..Tint::NONE
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_rise_fall_fade_and_are_removed() {
        let mut fx = Fx::new(1);
        fx.confetti_fountain(vec3(0., 1., 0.), 50, 8.);
        assert_eq!(fx.particles.len(), 50);
        let peak = fx
            .particles
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::MIN, f32::max);
        for _ in 0..30 {
            fx.update(1. / 60.);
        }
        let later = fx
            .particles
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::MIN, f32::max);
        assert!(later > peak, "confetti rises first");
        for _ in 0..(5 * 60) {
            fx.update(1. / 60.);
        }
        assert!(fx.particles.is_empty(), "all confetti expires");
    }

    #[test]
    fn particle_count_is_capped() {
        let mut fx = Fx::new(2);
        for _ in 0..200 {
            fx.confetti_fountain(Vec3::ZERO, 100, 6.);
        }
        assert!(fx.particles.len() <= MAX_PARTICLES);
    }

    #[test]
    fn a_cone_goes_where_it_is_aimed_and_a_seed_replays() {
        let mut fx = Fx::new(3);
        fx.confetti_cone(Vec3::ZERO, vec3(0., 0., -1.), 0.25, 10., 100);
        let mean: Vec3 = fx.particles.iter().map(|p| p.vel.normalize()).sum::<Vec3>() / 100.;
        assert!(mean.z < -0.85, "mean direction {mean:?}");
        let mut again = Fx::new(3);
        again.confetti_cone(Vec3::ZERO, vec3(0., 0., -1.), 0.25, 10., 100);
        assert_eq!(fx.particles[7].vel, again.particles[7].vel);
    }

    #[test]
    fn rings_expand_and_fade_over_their_life() {
        let mut fx = Fx::new(4);
        fx.ring(vec3(0., 0.1, 0.), Vec3::Y, 0.5, 6., 0.5, [1., 0.5, 0.]);
        let (mut a, mut b) = (Batch::new(), Batch::new());
        fx.draw(&mut a, &mut b, Vec3::Z * 5., Vec3::X, Vec3::Y);
        let first = a.meshes[0]
            .vertices
            .iter()
            .map(|v| v.position.x.abs())
            .fold(0., f32::max);
        for _ in 0..20 {
            fx.update(1. / 60.);
        }
        a.clear();
        fx.draw(&mut a, &mut b, Vec3::Z * 5., Vec3::X, Vec3::Y);
        let later = a.meshes[0]
            .vertices
            .iter()
            .map(|v| v.position.x.abs())
            .fold(0., f32::max);
        assert!(later > first * 2., "ring grew from {first} to {later}");
        for _ in 0..60 {
            fx.update(1. / 60.);
        }
        assert!(fx.rings.is_empty());
    }

    #[test]
    fn popups_rise_then_expire_and_banners_are_bounded() {
        let mut fx = Fx::new(5);
        fx.popup(vec3(0., 1., 0.), "+100", [1., 1., 0.], 30.);
        let y0 = fx.popups[0].pos.y;
        fx.update(0.3);
        assert!(fx.popups[0].pos.y > y0);
        fx.update(2.);
        assert!(fx.popups.is_empty());
        for i in 0..10 {
            fx.banner(format!("B{i}"), "", [1.; 3]);
        }
        assert_eq!(fx.banners.len(), 3);
        assert_eq!(fx.banners[2].text, "B9", "the newest survive");
        fx.clear();
        assert!(fx.banners.is_empty());
    }

    #[test]
    fn drawing_never_produces_invalid_geometry() {
        let mut fx = Fx::new(6);
        fx.confetti_fountain(vec3(1., 1., 1.), 200, 8.);
        fx.sparks(Vec3::ZERO, 100, 12., [1., 1., 0.5]);
        fx.dust(Vec3::ZERO, 30, 2., [0.5, 0.4, 0.3]);
        fx.ring(Vec3::ZERO, vec3(0.3, 1., 0.1), 0.2, 4., 0.6, [1.; 3]);
        fx.fireball(Vec3::ZERO, 3., 0.5, [1., 0.5, 0.1]);
        fx.beam(Vec3::ZERO, 0.9, [0.3, 1., 0.3]);
        for _ in 0..20 {
            fx.update(1. / 60.);
        }
        let (mut a, mut b) = (Batch::new(), Batch::new());
        fx.draw(&mut a, &mut b, Vec3::Z * 5., Vec3::X, Vec3::Y);
        assert!(a.vertex_count() > 0 && b.vertex_count() > 0);
        for m in a.meshes.iter().chain(&b.meshes) {
            assert!(m.vertices.iter().all(|v| v.position.is_finite()));
            assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
        }
    }
}
