use macroquad::prelude::*;
use std::f32::consts::TAU;
use sway::{audio::Features, Sway};

#[derive(Default)]
pub struct Visuals {
    pub time: f64,
    pub features: Features,
}
impl Visuals {
    pub fn advance(&mut self, dt: f32, next: Features, speed: f32) {
        self.time += f64::from(dt * speed);
        let k = 1. - (-dt * 12.).exp();
        for i in 0..32 {
            self.features.bands[i] += (next.bands[i] - self.features.bands[i]) * k;
        }
        for i in 0..64 {
            self.features.wave[i] += (next.wave[i] - self.features.wave[i]) * k;
        }
        self.features.energy += (next.energy - self.features.energy) * k;
        self.features.bass += (next.bass - self.features.bass) * k;
        self.features.mid += (next.mid - self.features.mid) * k;
        self.features.high += (next.high - self.features.high) * k;
        self.features.beat = next.beat.max(self.features.beat * (-dt * 5.).exp());
    }
    pub fn draw(&self, model: &Sway, viewport: (i32, i32, i32, i32)) {
        let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., 1280., 720.));
        camera.zoom.y = camera.zoom.y.abs();
        camera.viewport = Some(viewport);
        set_camera(&camera);
        let chapter = (self.time / 37.) as u64;
        let seed = model.key(chapter);
        let next = model.key(chapter + 1);
        let u = ((self.time % 37.) as f32 / 37. - 0.78).max(0.) / 0.22;
        let blend = u * u * (3. - 2. * u);
        let palette = if model.state.palette == 0 {
            ((seed >> 16) % 6 + 1) as u8
        } else {
            model.state.palette
        };
        let hue = base_hue(palette) + unit(seed >> 32) * 0.12;
        for i in 0..24 {
            let v = i as f32 / 24.;
            let mut c = color(
                hue + v * 0.09,
                0.4,
                0.025 + 0.016 * (v * std::f32::consts::PI).sin(),
                1.,
            );
            c.b += 0.01;
            draw_rectangle(0., i as f32 * 30., 1280., 31., c);
        }
        // Low-cost luminous dust, with no fullscreen fragment march or postprocessing.
        for i in 0..90 {
            let h = sway::mix(seed ^ (i * 7919));
            let x = unit(h) * 1280.;
            let y = (unit(h >> 20) * 720. + self.time as f32 * (3. + unit(h >> 40) * 9.))
                .rem_euclid(720.);
            draw_circle(
                x,
                y,
                0.6 + unit(h >> 32),
                color(hue + unit(h >> 8) * 0.2, 0.35, 0.65, 0.12),
            );
        }
        let a = if model.state.style == 0 {
            (seed % 6) as usize
        } else {
            model.state.style as usize - 1
        };
        let b = if model.state.style == 0 {
            (next % 6) as usize
        } else {
            a
        };
        self.shape(a, seed, palette, 1. - blend);
        if blend > 0.001 {
            self.shape(
                b,
                next,
                if model.state.palette == 0 {
                    ((next >> 16) % 6 + 1) as u8
                } else {
                    palette
                },
                blend,
            );
        }
        set_default_camera();
    }
    fn shape(&self, style: usize, seed: u64, palette: u8, alpha: f32) {
        if alpha < 0.001 {
            return;
        }
        let t = self.time as f32;
        let f = self.features;
        let scale = 1. + f.bass * 0.12 + f.beat * 0.08;
        let center = vec2(
            640. + (t * 0.071 + unit(seed) * TAU).sin() * 32.,
            360. + (t * 0.053).cos() * 20.,
        );
        let spin = if seed & 1 == 0 { 1. } else { -1. };
        let hue = base_hue(palette) + unit(seed >> 32) * 0.12 + t * 0.004;
        let rgb = |u: f32, a: f32| {
            color(
                hue + u * if palette == 6 { 0.02 } else { 0.35 },
                if palette == 6 { 0.15 } else { 0.68 },
                1.,
                a * alpha,
            )
        };
        match style {
            0 => {
                for layer in 0..7 {
                    let radius = (92. + layer as f32 * 27.) * scale;
                    let mut points = Vec::with_capacity(129);
                    for i in 0..=128 {
                        let a = i as f32 / 128. * TAU;
                        let band = f.bands[(i + layer * 3) % 32];
                        let r = radius
                            + band * (18. + layer as f32 * 7.)
                            + (a * (3. + (seed % 5) as f32) + t * 0.55 * spin + layer as f32).sin()
                                * 12.;
                        points.push(center + vec2(a.cos() * r, a.sin() * r * 0.88));
                    }
                    stroke(&points, rgb(layer as f32 / 8., 0.7), 1.3);
                    let a = t * (0.12 + layer as f32 * 0.035) * spin + layer as f32 * 0.9;
                    let p = center + vec2(a.cos() * radius, a.sin() * radius * 0.88);
                    draw_circle(p.x, p.y, 2.5 + f.high * 3., rgb(layer as f32 / 8., 0.85));
                }
            }
            1 => {
                for layer in 0..9 {
                    let mut points = Vec::with_capacity(128);
                    for i in 0..128 {
                        let u = i as f32 / 127.;
                        let x = 80. + u * 1120.;
                        let envelope = (u * std::f32::consts::PI).sin();
                        let y = 360.
                            + envelope
                                * ((u * TAU * (1.5 + unit(seed) * 2.)
                                    + t * 0.55
                                    + layer as f32 * 0.3)
                                    .sin()
                                    * (75. + layer as f32 * 13.)
                                    + (u * TAU * 4. - t * 0.33 + layer as f32).cos() * f.mid * 50.
                                    + f.wave[i % 64] * 48.)
                            + (layer as f32 - 4.) * 7.;
                        points.push(vec2(x, y));
                    }
                    stroke(&points, rgb(layer as f32 / 10., 0.55), 1.4);
                }
            }
            2 => {
                let sides = 3 + (seed % 5) as usize;
                for layer in 0..14 {
                    let a = t * 0.12 * spin + layer as f32 * 0.17;
                    let r = (48. + layer as f32 * 17. + f.bands[(layer * 2) % 32] * 45.) * scale;
                    let points: Vec<_> = (0..=sides)
                        .map(|n| {
                            let angle = a + n as f32 / sides as f32 * TAU;
                            center + vec2(angle.cos() * r, angle.sin() * r)
                        })
                        .collect();
                    stroke(&points, rgb(layer as f32 / 14., 0.6), 1.5);
                }
            }
            3 => {
                let petals = 4. + (seed % 7) as f32;
                for layer in 0..9 {
                    let mut points = Vec::with_capacity(193);
                    for i in 0..=192 {
                        let a = i as f32 / 192. * TAU;
                        let r = (68.
                            + layer as f32 * 18.
                            + (a * petals + t * 0.2 * spin + layer as f32 * 0.18).sin()
                                * (21. + f.bass * 38.)
                            + f.bands[i % 32] * 25.)
                            * scale;
                        let angle = a + t * 0.09 * spin;
                        points.push(center + vec2(angle.cos() * r, angle.sin() * r));
                    }
                    stroke(&points, rgb(layer as f32 / 9., 0.6), 1.3);
                }
            }
            4 => {
                for i in 0..280_u64 {
                    let h = sway::mix(seed ^ i.wrapping_mul(0x9e3779b97f4a7c15));
                    let z = (unit(h) * 1.5 - t * (0.025 + f.bass * 0.026)).rem_euclid(1.5) + 0.2;
                    let angle = unit(h >> 16) * TAU + t * 0.045 * spin;
                    let r = (60. + unit(h >> 32) * 190.) / z;
                    let p = center + vec2(angle.cos() * r, angle.sin() * r);
                    let q = center + (p - center) * (1. - 0.025 - f.high * 0.04);
                    let c = rgb(unit(h >> 40), ((1.7 - z) / 1.5).clamp(0., 1.) * 0.8);
                    glow_line(
                        q,
                        p,
                        c,
                        0.8 + (1.7 - z) * 1.3 + f.bands[i as usize % 32] * 1.8,
                    );
                }
            }
            _ => {
                for layer in 0..11 {
                    let mut points = Vec::with_capacity(128);
                    for i in 0..128 {
                        let u = i as f32 / 127.;
                        let envelope = (u * std::f32::consts::PI).sin();
                        let y = 180.
                            + layer as f32 * 34.
                            + envelope
                                * ((u * TAU * 2. + t * 0.3 + layer as f32 * 0.3).sin()
                                    * (18. + f.bass * 30.)
                                    + f.wave[i % 64] * 25.
                                    + f.bands[i % 32] * 30. * (u * TAU * 8. - t).sin());
                        points.push(vec2(70. + u * 1140., y));
                    }
                    stroke(&points, rgb(layer as f32 / 11., 0.55), 1.2);
                }
            }
        }
    }
}
fn unit(v: u64) -> f32 {
    (v & 65535) as f32 / 65535.
}
fn base_hue(p: u8) -> f32 {
    match p {
        1 => 0.49,
        2 => 0.99,
        3 => 0.70,
        4 => 0.40,
        5 => 0.07,
        _ => 0.56,
    }
}
fn color(h: f32, s: f32, v: f32, a: f32) -> Color {
    let h = h.rem_euclid(1.) * 6.;
    let c = v * s;
    let x = c * (1. - (h % 2. - 1.).abs());
    let m = v - c;
    let (r, g, b) = match h as usize {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    Color::new(r + m, g + m, b + m, a)
}
fn glow_line(a: Vec2, b: Vec2, c: Color, width: f32) {
    draw_line(
        a.x,
        a.y,
        b.x,
        b.y,
        width + 5.,
        Color::new(c.r, c.g, c.b, c.a * 0.09),
    );
    draw_line(a.x, a.y, b.x, b.y, width, c);
}
fn stroke(points: &[Vec2], c: Color, width: f32) {
    for pair in points.windows(2) {
        glow_line(pair[0], pair[1], c, width);
    }
}
