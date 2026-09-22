use crate::{
    geometry::{Compiled, World},
    math::*,
    scene::{Ease, Scene},
    Result,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug)]
pub struct Quality {
    pub samples: u32,
    pub shadows: u32,
    pub ao: u32,
    pub bounces: u32,
}
impl Quality {
    pub fn named(name: &str) -> Result<Self> {
        Ok(match name {
            "draft" => Self {
                samples: 1,
                shadows: 1,
                ao: 0,
                bounces: 1,
            },
            "standard" => Self {
                samples: 4,
                shadows: 1,
                ao: 1,
                bounces: 1,
            },
            "high" => Self {
                samples: 9,
                shadows: 2,
                ao: 2,
                bounces: 2,
            },
            "ultra" => Self {
                samples: 25,
                shadows: 4,
                ao: 4,
                bounces: 2,
            },
            _ => return Err("quality must be draft, standard, high, or ultra".into()),
        })
    }
}
pub struct Options {
    pub quality: Quality,
    pub threads: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            quality: Quality::named("standard").unwrap(),
            threads: std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(32),
        }
    }
}
fn hemisphere(n: V, seed: u32) -> V {
    let a = random(seed) * std::f32::consts::TAU;
    let z = random(seed.wrapping_add(177));
    let radius = z.sqrt();
    let tangent = if n.1.abs() < 0.99 {
        V(0., 1., 0.).cross(n).norm()
    } else {
        V(1., 0., 0.).cross(n).norm()
    };
    let bitangent = n.cross(tangent);
    (tangent * (radius * a.cos()) + bitangent * (radius * a.sin()) + n * (1. - z).sqrt()).norm()
}
fn background(scene: &Scene, d: V) -> V {
    scene
        .world
        .horizon
        .lerp(scene.world.sky, (d.1 * 0.7 + 0.35).clamp(0., 1.))
}
struct Context<'a> {
    world: &'a World,
    scene: &'a Scene,
    lights: Vec<(V, V, f32, f32)>,
    quality: Quality,
}
impl Context<'_> {
    fn trace(&self, r: Ray, depth: u32, seed: u32) -> V {
        let Some(h) = self.world.hit(r, 5000., false) else {
            return background(self.scene, r.d);
        };
        let m = &self.world.instances[h.index].material;
        let mut albedo = m.color;
        if let Some(c) = m.checker {
            if (h.p.0.floor() as i32 + h.p.2.floor() as i32).rem_euclid(2) == 0 {
                albedo = c;
            }
        }
        let view = -r.d;
        let nv = h.n.dot(view).max(0.001);
        let f0 = V::ONE * 0.04 * (1. - m.metallic) + albedo * m.metallic;
        let mut occlusion = 1.;
        if depth == 0 && self.quality.ao > 0 {
            let mut blocked = 0.;
            for i in 0..self.quality.ao {
                let direction = hemisphere(h.n, seed.wrapping_add(i * 31 + 71));
                if let Some(a) = self.world.hit(
                    Ray {
                        o: h.p + h.n * 0.002,
                        d: direction,
                    },
                    1.8,
                    true,
                ) {
                    blocked += 1. - a.t / 1.8;
                }
            }
            occlusion = 1. - 0.65 * blocked / self.quality.ao as f32;
        }
        let mut color = albedo
            * (self.scene.world.ambient
                * (0.6 + 0.4 * h.n.1.max(0.))
                * occlusion
                * (1. - 0.65 * m.metallic))
            + albedo * m.emission;
        for (li, (position, light_color, power, radius)) in self.lights.iter().enumerate() {
            for sample in 0..self.quality.shadows {
                let s = seed.wrapping_add(li as u32 * 739 + sample * 113);
                let jitter = V(
                    random(s) - 0.5,
                    random(s.wrapping_add(19)) - 0.5,
                    random(s.wrapping_add(47)) - 0.5,
                ) * (*radius * 2.);
                let delta = *position + jitter - h.p;
                let distance = delta.length();
                let l = delta / distance.max(0.001);
                let nl = h.n.dot(l).max(0.);
                if nl <= 0. {
                    continue;
                }
                if self
                    .world
                    .hit(
                        Ray {
                            o: h.p + h.n * 0.002,
                            d: l,
                        },
                        distance - 0.003,
                        true,
                    )
                    .is_some()
                {
                    continue;
                }
                let half = (l + view).norm();
                let nh = h.n.dot(half).max(0.);
                let vh = view.dot(half).max(0.);
                let a = m.roughness * m.roughness;
                let a2 = a * a;
                let den = nh * nh * (a2 - 1.) + 1.;
                let d = a2 / (std::f32::consts::PI * den * den).max(0.000001);
                let k = (m.roughness + 1.).powi(2) / 8.;
                let g = (nl / (nl * (1. - k) + k)) * (nv / (nv * (1. - k) + k));
                let f = f0 + (V::ONE - f0) * (1. - vh).powi(5);
                let spec = f * (d * g / (4. * nl * nv).max(0.00001));
                let diffuse = (V::ONE - f) * albedo * ((1. - m.metallic) / std::f32::consts::PI);
                color = color
                    + (diffuse + spec)
                        * (*light_color)
                        * (*power / (distance * distance).max(0.2))
                        * nl
                        / self.quality.shadows as f32;
            }
        }
        if depth < self.quality.bounces && (m.metallic > 0.01 || m.roughness < 0.3) {
            let reflection = r.d.reflect(h.n);
            let direction = reflection;
            let f = f0 + (V::ONE - f0) * (1. - nv).powi(5);
            color = color
                + self.trace(
                    Ray {
                        o: h.p + h.n * 0.003,
                        d: direction,
                    },
                    depth + 1,
                    seed.wrapping_add(713),
                ) * f
                    * (1. - m.roughness * 0.55);
        }
        color.lerp(
            background(self.scene, r.d),
            1. - (-self.scene.world.fog * h.t).exp(),
        )
    }
}
fn camera(scene: &Scene, t: f32) -> Result<(V, V, V, V)> {
    let target = scene.camera.target.at(t, scene.camera.ease);
    let mut eye = scene.camera.pos.at(t, scene.camera.ease);
    if scene.camera.orbit != 0. {
        let d = eye - target;
        let a = (scene.camera.orbit * t).to_radians();
        eye = target
            + V(
                d.0 * a.cos() + d.2 * a.sin(),
                d.1,
                -d.0 * a.sin() + d.2 * a.cos(),
            );
    }
    let forward = (target - eye).norm();
    if (target - eye).length() < 0.001 {
        return Err("camera position coincides with target at render time".into());
    }
    let up = if forward.1.abs() > 0.999 {
        V(0., 0., 1.)
    } else {
        V(0., 1., 0.)
    };
    let right = forward.cross(up).norm();
    Ok((eye, forward, right, right.cross(forward)))
}
pub fn frame(
    compiled: &Compiled,
    t: f32,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    if !t.is_finite() || t < 0. || t > compiled.scene.duration {
        return Err("frame time is outside the scene duration".into());
    }
    if !(1..=64).contains(&options.threads) {
        return Err("threads must be 1..64".into());
    }
    let q = options.quality;
    if !matches!(q.samples, 1 | 4 | 9 | 25)
        || !(1..=16).contains(&q.shadows)
        || q.ao > 16
        || q.bounces > 4
    {
        return Err("invalid sample/shadow/AO/reflection budget".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("render cancelled".into());
    }
    let s = &compiled.scene;
    let w = s.size[0] as usize;
    let h = s.size[1] as usize;
    let world = compiled.at(t);
    let ctx = Context {
        world: &world,
        scene: s,
        lights: s
            .lights
            .iter()
            .map(|l| (l.pos.at(t, Ease::Smooth), l.color, l.power, l.radius))
            .collect(),
        quality: options.quality,
    };
    let (eye, forward, right, up) = camera(s, t)?;
    let lens = (s.camera.fov.to_radians() * 0.5).tan();
    let aspect = w as f32 / h as f32;
    let count = options.quality.samples;
    let grid = (count as f32).sqrt() as u32;
    let mut pixels = vec![V::ZERO; w * h];
    let chunk = (h.div_ceil(options.threads)) * w;
    std::thread::scope(|scope| {
        for (block, rows) in pixels.chunks_mut(chunk).enumerate() {
            let ctx = &ctx;
            scope.spawn(move || {
                for (i, pixel) in rows.iter_mut().enumerate() {
                    let index = block * chunk + i;
                    let x = index % w;
                    let y = index / w;
                    if x == 0 && cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut sum = V::ZERO;
                    for sample in 0..count {
                        let seed =
                            hash((index as u32).wrapping_mul(997).wrapping_add(sample * 239));
                        let jx = (sample % grid) as f32 + 0.5;
                        let jy = (sample / grid) as f32 + 0.5;
                        let u =
                            (2. * (x as f32 + jx / grid as f32) / w as f32 - 1.) * aspect * lens;
                        let v = (1. - 2. * (y as f32 + jy / grid as f32) / h as f32) * lens;
                        let mut direction = (forward + right * u + up * v).norm();
                        let mut origin = eye;
                        if s.camera.aperture > 0. {
                            let a = random(seed) * std::f32::consts::TAU;
                            let radius = random(seed.wrapping_add(21)).sqrt() * s.camera.aperture;
                            origin = eye + (right * a.cos() + up * a.sin()) * radius;
                            let focus = eye
                                + direction * (s.camera.focus / direction.dot(forward).max(0.001));
                            direction = (focus - origin).norm();
                        }
                        sum = sum
                            + ctx.trace(
                                Ray {
                                    o: origin,
                                    d: direction,
                                },
                                0,
                                seed,
                            );
                    }
                    *pixel = sum / count as f32;
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err("render cancelled".into());
    }
    Ok(finish(&pixels, w, h, s.world.exposure, s.world.bloom))
}
fn finish(pixels: &[V], w: usize, h: usize, exposure: f32, bloom: f32) -> Vec<u8> {
    let mut glow = vec![V::ZERO; pixels.len()];
    let mut temp = vec![V::ZERO; pixels.len()];
    if bloom > 0. {
        for (g, p) in glow.iter_mut().zip(pixels) {
            *g = (*p - V::ONE).max(V::ZERO);
        }
        let radius = (w / 160).clamp(2, 16) as isize;
        for pass in 0..2 {
            for y in 0..h {
                for x in 0..w {
                    let mut sum = V::ZERO;
                    let mut weight = 0.;
                    for d in -radius..=radius {
                        let (xx, yy) = if pass == 0 {
                            ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                        } else {
                            (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                        };
                        let k = (radius + 1 - d.abs()) as f32;
                        sum = sum + glow[yy * w + xx] * k;
                        weight += k;
                    }
                    temp[y * w + x] = sum / weight;
                }
            }
            std::mem::swap(&mut glow, &mut temp);
        }
    }
    let aces = |x: f32| {
        let x = x.max(0.);
        let y = (x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0., 1.);
        let srgb = if y <= 0.0031308 {
            12.92 * y
        } else {
            1.055 * y.powf(1. / 2.4) - 0.055
        };
        (srgb * 255. + 0.5) as u8
    };
    let mut out = Vec::with_capacity(w * h * 3);
    for (i, p) in pixels.iter().enumerate() {
        let x = (i % w) as f32 / w as f32 - 0.5;
        let y = (i / w) as f32 / h as f32 - 0.5;
        let vignette = 1. - 0.22 * (x * x + y * y);
        let c = (*p + glow[i] * bloom) * (exposure * vignette);
        out.extend_from_slice(&[aces(c.0), aces(c.1), aces(c.2)]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_during_render() {
        let s = Scene {
            size: [256, 256],
            ..Scene::default()
        };
        let c = Compiled::new(s, Path::new(".")).unwrap();
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(2));
                cancel.store(true, Ordering::Relaxed);
            });
            let result = frame(
                &c,
                0.,
                &Options {
                    quality: Quality::named("ultra").unwrap(),
                    threads: 1,
                },
                &cancel,
            );
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        });
    }
    #[test]
    fn invalid_quality_is_rejected() {
        let c = Compiled::new(Scene::default(), Path::new(".")).unwrap();
        let options = Options {
            quality: Quality {
                samples: 0,
                shadows: 0,
                ao: 0,
                bounces: 0,
            },
            threads: 1,
        };
        assert!(frame(&c, 0., &options, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn deterministic_across_thread_counts() {
        let mut s = Scene {
            size: [32, 24],
            ..Scene::default()
        };
        s.nodes
            .push(serde_json::from_str(r#"{"id":"ball","shape":"sphere"}"#).unwrap());
        let c = Compiled::new(s, Path::new(".")).unwrap();
        let cancel = AtomicBool::new(false);
        let a = frame(
            &c,
            0.,
            &Options {
                quality: Quality::named("standard").unwrap(),
                threads: 1,
            },
            &cancel,
        )
        .unwrap();
        let b = frame(
            &c,
            0.,
            &Options {
                quality: Quality::named("standard").unwrap(),
                threads: 3,
            },
            &cancel,
        )
        .unwrap();
        assert_eq!(a, b);
        assert!(a.iter().any(|v| *v > 0));
    }
    #[test]
    fn cancellation_and_degenerate_camera() {
        let mut s = Scene {
            size: [16, 16],
            ..Scene::default()
        };
        s.camera.pos = s.camera.target.clone();
        let c = Compiled::new(s, Path::new(".")).unwrap();
        assert!(frame(&c, 0., &Options::default(), &AtomicBool::new(false)).is_err());
        assert!(frame(&c, 0., &Options::default(), &AtomicBool::new(true)).is_err());
    }
    use std::path::Path;
}
