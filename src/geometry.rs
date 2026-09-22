use crate::{math::*, scene::*, Result};
use std::{path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub enum Primitive {
    Sphere,
    Box,
    Cylinder,
    Cone,
    Triangle([V; 3], [V; 3]),
}
#[derive(Clone, Debug)]
pub struct Part {
    pub shape: Primitive,
    pub local: Mat,
    pub material: Material,
    pub limb: i32,
}
#[derive(Clone)]
pub struct Instance {
    pub shape: Primitive,
    pub inverse: Mat,
    pub bounds: Bounds,
    pub material: Material,
}
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub p: V,
    pub n: V,
    pub index: usize,
}

impl Instance {
    pub fn new(part: &Part, world: Mat) -> Self {
        let m = world.compose(part.local);
        let mut b = Bounds::empty();
        if let Primitive::Triangle(v, _) = &part.shape {
            for p in v {
                b = b.include(m.point(*p));
            }
        } else {
            for x in [-1., 1.] {
                for y in [-1., 1.] {
                    for z in [-1., 1.] {
                        b = b.include(m.point(V(x, y, z)));
                    }
                }
            }
        }
        b.lo = b.lo - V::ONE * 0.0001;
        b.hi = b.hi + V::ONE * 0.0001;
        Self {
            shape: part.shape.clone(),
            inverse: m.inverse(),
            bounds: b,
            material: part.material.clone(),
        }
    }
    pub fn intersect(&self, ray: Ray, max: f32) -> Option<(f32, V)> {
        let r = Ray {
            o: self.inverse.point(ray.o),
            d: self.inverse.vector(ray.d),
        };
        let mut best = max;
        let mut normal = V::ZERO;
        let mut accept = |t: f32, n: V| {
            if t > 0.0005 && t < best {
                best = t;
                normal = n;
            }
        };
        match &self.shape {
            Primitive::Sphere => {
                let a = r.d.dot(r.d);
                let b = r.o.dot(r.d);
                let c = r.o.dot(r.o) - 1.;
                let disc = b * b - a * c;
                if disc >= 0. {
                    let q = disc.sqrt();
                    for t in [(-b - q) / a, (-b + q) / a] {
                        accept(t, r.at(t));
                    }
                }
            }
            Primitive::Box => {
                let mut lo = f32::NEG_INFINITY;
                let mut hi = f32::INFINITY;
                let mut ln = V::ZERO;
                let mut hn = V::ZERO;
                for i in 0..3 {
                    let d = r.d.axis(i);
                    let o = r.o.axis(i);
                    if d.abs() < 1e-12 {
                        if o.abs() > 1. {
                            return None;
                        }
                        continue;
                    }
                    let a = (-1. - o) / d;
                    let b = (1. - o) / d;
                    let axis = match i {
                        0 => V(1., 0., 0.),
                        1 => V(0., 1., 0.),
                        _ => V(0., 0., 1.),
                    };
                    let (l, h, n) = if a < b { (a, b, -axis) } else { (b, a, axis) };
                    if l > lo {
                        lo = l;
                        ln = n;
                    }
                    if h < hi {
                        hi = h;
                        hn = -n;
                    }
                    if lo > hi {
                        return None;
                    }
                }
                accept(lo, ln);
                accept(hi, hn);
            }
            Primitive::Cylinder | Primitive::Cone => {
                let cone = matches!(self.shape, Primitive::Cone);
                let k = if cone { 0.25 } else { 0. };
                let a = r.d.0 * r.d.0 + r.d.2 * r.d.2 - k * r.d.1 * r.d.1;
                let b = 2. * (r.o.0 * r.d.0 + r.o.2 * r.d.2 + k * (1. - r.o.1) * r.d.1);
                let c = r.o.0 * r.o.0 + r.o.2 * r.o.2
                    - if cone {
                        0.25 * (1. - r.o.1).powi(2)
                    } else {
                        1.
                    };
                let disc = b * b - 4. * a * c;
                let roots = if a.abs() < 1e-10 {
                    if b.abs() > 1e-10 {
                        [-c / b, -c / b]
                    } else {
                        [f32::NAN; 2]
                    }
                } else if disc >= 0. {
                    [(-b - disc.sqrt()) / (2. * a), (-b + disc.sqrt()) / (2. * a)]
                } else {
                    [f32::NAN; 2]
                };
                for t in roots {
                    let p = r.at(t);
                    if p.1 >= -1. && p.1 <= 1. {
                        accept(t, V(p.0, if cone { 0.25 * (1. - p.1) } else { 0. }, p.2));
                    }
                }
                if r.d.1.abs() > 1e-12 {
                    for y in [-1., 1.] {
                        let t = (y - r.o.1) / r.d.1;
                        let p = r.at(t);
                        let radius = if cone && y > 0. { 0. } else { 1. };
                        if p.0 * p.0 + p.2 * p.2 <= radius {
                            accept(t, V(0., y, 0.));
                        }
                    }
                }
            }
            Primitive::Triangle(v, n) => {
                let e1 = v[1] - v[0];
                let e2 = v[2] - v[0];
                let h = r.d.cross(e2);
                let det = e1.dot(h);
                if det.abs() < 1e-9 {
                    return None;
                }
                let f = 1. / det;
                let s = r.o - v[0];
                let u = f * s.dot(h);
                if !(0. ..=1.).contains(&u) {
                    return None;
                }
                let q = s.cross(e1);
                let v2 = f * r.d.dot(q);
                if v2 < 0. || u + v2 > 1. {
                    return None;
                }
                accept(f * e2.dot(q), n[0] * (1. - u - v2) + n[1] * u + n[2] * v2);
            }
        }
        if best < max {
            let mut n = self.inverse.normal_from_inverse(normal);
            if n.dot(ray.d) > 0. {
                n = -n;
            }
            Some((best, n))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct BvhNode {
    bounds: Bounds,
    left: usize,
    right: usize,
    start: usize,
    end: usize,
}
pub struct World {
    pub instances: Vec<Instance>,
    order: Vec<usize>,
    nodes: Vec<BvhNode>,
}
impl World {
    pub fn new(instances: Vec<Instance>) -> Self {
        let mut s = Self {
            order: (0..instances.len()).collect(),
            instances,
            nodes: vec![],
        };
        if !s.order.is_empty() {
            s.build(0, s.order.len());
        }
        s
    }
    fn build(&mut self, start: usize, end: usize) -> usize {
        let mut bounds = Bounds::empty();
        for i in &self.order[start..end] {
            bounds = bounds.union(self.instances[*i].bounds);
        }
        let index = self.nodes.len();
        self.nodes.push(BvhNode {
            bounds,
            left: 0,
            right: 0,
            start,
            end,
        });
        if end - start > 4 {
            let d = bounds.hi - bounds.lo;
            let axis = if d.0 > d.1 && d.0 > d.2 {
                0
            } else if d.1 > d.2 {
                1
            } else {
                2
            };
            let instances = &self.instances;
            self.order[start..end].sort_unstable_by(|a, b| {
                let a = instances[*a].bounds;
                let b = instances[*b].bounds;
                (a.lo + a.hi)
                    .axis(axis)
                    .total_cmp(&(b.lo + b.hi).axis(axis))
            });
            let mid = (start + end) / 2;
            let left = self.build(start, mid);
            let right = self.build(mid, end);
            self.nodes[index].left = left;
            self.nodes[index].right = right;
        }
        index
    }
    pub fn hit(&self, r: Ray, max: f32, any: bool) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut stack = [0usize; 64];
        let mut top = 1;
        let mut nearest = max;
        let mut result = None;
        while top > 0 {
            top -= 1;
            let node = &self.nodes[stack[top]];
            if !node.bounds.hit(r, 0.0005, nearest) {
                continue;
            }
            if node.left == 0 {
                for i in &self.order[node.start..node.end] {
                    if let Some((t, n)) = self.instances[*i].intersect(r, nearest) {
                        let hit = Hit {
                            t,
                            p: r.at(t),
                            n,
                            index: *i,
                        };
                        if any {
                            return Some(hit);
                        }
                        nearest = t;
                        result = Some(hit);
                    }
                }
            } else {
                stack[top] = node.right;
                stack[top + 1] = node.left;
                top += 2;
            }
        }
        result
    }
}

pub struct Compiled {
    pub scene: Scene,
    parts: Vec<Arc<Vec<Part>>>,
}
fn part(shape: Primitive, p: V, s: V, m: &Material) -> Part {
    Part {
        shape,
        local: Mat::trs(p, V::ZERO, s),
        material: m.clone(),
        limb: 0,
    }
}
fn colored(c: V, metallic: f32, roughness: f32, emission: f32) -> Material {
    Material {
        color: c,
        metallic,
        roughness,
        emission,
        checker: None,
    }
}
fn triangle(v: [V; 3], normals: Option<[V; 3]>, m: &Material) -> Part {
    let n = (v[1] - v[0]).cross(v[2] - v[0]).norm();
    Part {
        shape: Primitive::Triangle(v, normals.unwrap_or([n; 3])),
        local: Mat::identity(),
        material: m.clone(),
        limb: 0,
    }
}
fn torus(m: &Material, tube: f32) -> Vec<Part> {
    let mut out = vec![];
    let nu = 48;
    let nv = 12;
    let vertex = |i: usize, j: usize| {
        let a = i as f32 / nu as f32 * std::f32::consts::TAU;
        let b = j as f32 / nv as f32 * std::f32::consts::TAU;
        let n = V(a.cos() * b.cos(), b.sin(), a.sin() * b.cos());
        (
            V(a.cos() * (1. - tube), 0., a.sin() * (1. - tube)) + n * tube,
            n,
        )
    };
    for i in 0..nu {
        for j in 0..nv {
            let a = vertex(i, j);
            let b = vertex(i + 1, j);
            let c = vertex(i + 1, j + 1);
            let d = vertex(i, j + 1);
            out.push(triangle([a.0, b.0, c.0], Some([a.1, b.1, c.1]), m));
            out.push(triangle([a.0, c.0, d.0], Some([a.1, c.1, d.1]), m));
        }
    }
    out
}
fn prefab(shape: Shape, m: &Material) -> Vec<Part> {
    let dark = colored(V(0.018, 0.028, 0.048), 0.6, 0.27, 0.);
    let glow = colored(V(0.08, 0.85, 1.), 0.2, 0.18, 3.);
    let gold = colored(V(0.95, 0.46, 0.085), 0.65, 0.22, 0.);
    match shape {
        Shape::Group | Shape::Mesh => vec![],
        Shape::Sphere => vec![part(Primitive::Sphere, V::ZERO, V::ONE, m)],
        Shape::Box => vec![part(Primitive::Box, V::ZERO, V::ONE, m)],
        Shape::Cylinder => vec![part(Primitive::Cylinder, V::ZERO, V::ONE, m)],
        Shape::Cone => vec![part(Primitive::Cone, V::ZERO, V::ONE, m)],
        Shape::Torus => torus(m, 0.22),
        Shape::Crystal => {
            let mut out = vec![];
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 6.;
                let p = V(a.cos() * 0.7, 0., a.sin() * 0.7);
                let q = V(b.cos() * 0.7, 0., b.sin() * 0.7);
                out.push(triangle([p, q, V(0., 1.4, 0.)], None, m));
                out.push(triangle([q, p, V(0., -0.8, 0.)], None, m));
            }
            out
        }
        Shape::Tree => {
            let wood = colored(V(0.22, 0.095, 0.04), 0., 0.9, 0.);
            vec![
                part(
                    Primitive::Cylinder,
                    V(0., 0.6, 0.),
                    V(0.15, 0.6, 0.15),
                    &wood,
                ),
                part(Primitive::Cone, V(0., 1.2, 0.), V(0.95, 0.8, 0.95), m),
                part(Primitive::Cone, V(0., 1.85, 0.), V(0.72, 0.7, 0.72), m),
                part(Primitive::Cone, V(0., 2.4, 0.), V(0.46, 0.6, 0.46), m),
            ]
        }
        Shape::Robot => {
            let mut v = vec![
                part(Primitive::Sphere, V(0., 1.05, 0.), V(0.45, 0.51, 0.3), m),
                part(Primitive::Sphere, V(0., 1.77, 0.), V(0.55, 0.43, 0.38), m),
                part(
                    Primitive::Sphere,
                    V(0., 1.79, 0.285),
                    V(0.45, 0.28, 0.14),
                    &dark,
                ),
                part(
                    Primitive::Sphere,
                    V(-0.17, 1.82, 0.405),
                    V(0.061, 0.083, 0.03),
                    &glow,
                ),
                part(
                    Primitive::Sphere,
                    V(0.17, 1.82, 0.405),
                    V(0.061, 0.083, 0.03),
                    &glow,
                ),
                part(
                    Primitive::Cylinder,
                    V(0., 2.22, 0.),
                    V(0.032, 0.12, 0.032),
                    &dark,
                ),
                part(Primitive::Sphere, V(0., 2.37, 0.), V::ONE * 0.077, &glow),
                part(
                    Primitive::Sphere,
                    V(0., 1.12, 0.29),
                    V(0.13, 0.13, 0.038),
                    &gold,
                ),
            ];
            for side in [-1., 1.] {
                let mut arm = part(
                    Primitive::Sphere,
                    V(side * 0.58, 1.08, 0.),
                    V(0.12, 0.38, 0.14),
                    m,
                );
                arm.limb = if side < 0. { -1 } else { 1 };
                v.push(arm);
                let mut leg = part(
                    Primitive::Sphere,
                    V(side * 0.23, 0.38, 0.),
                    V(0.18, 0.35, 0.2),
                    &dark,
                );
                leg.limb = if side < 0. { -2 } else { 2 };
                v.push(leg);
                let mut foot = part(
                    Primitive::Sphere,
                    V(side * 0.23, 0.14, 0.1),
                    V(0.23, 0.14, 0.33),
                    m,
                );
                foot.limb = if side < 0. { -2 } else { 2 };
                v.push(foot);
            }
            v
        }
        Shape::Rocket => {
            let mut v = vec![
                part(Primitive::Cylinder, V(0., 1.1, 0.), V(0.47, 0.85, 0.47), m),
                part(Primitive::Cone, V(0., 2.35, 0.), V(0.47, 0.42, 0.47), &gold),
                part(
                    Primitive::Sphere,
                    V(0., 1.42, 0.43),
                    V(0.21, 0.21, 0.06),
                    &dark,
                ),
                part(
                    Primitive::Sphere,
                    V(0., 1.42, 0.478),
                    V(0.16, 0.16, 0.02),
                    &glow,
                ),
            ];
            for x in [-1., 1.] {
                v.push(part(
                    Primitive::Sphere,
                    V(x * 0.5, 0.4, 0.),
                    V(0.18, 0.5, 0.28),
                    &gold,
                ));
            }
            v
        }
    }
}
fn load_obj(path: &Path, m: &Material) -> Result<Vec<Part>> {
    if std::fs::metadata(path)?.len() > 32 * 1024 * 1024 {
        return Err("OBJ exceeds 32 MiB".into());
    }
    let text = std::fs::read_to_string(path)?;
    let mut vertices = vec![];
    let mut parts = vec![];
    for (line_no, line) in text.lines().enumerate() {
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("v") => {
                let values: Vec<f32> = words
                    .take(3)
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?;
                if values.len() != 3 {
                    return Err(format!("OBJ line {}: expected 3 coordinates", line_no + 1).into());
                }
                let v = V(values[0], values[1], values[2]);
                if !v.finite() || v.length() > 10000. {
                    return Err("OBJ coordinate out of range".into());
                }
                vertices.push(v);
            }
            Some("f") => {
                let indices: Vec<usize> = words
                    .map(|w| -> Result<usize> {
                        let i: i64 = w.split('/').next().unwrap_or("").parse()?;
                        let n = vertices.len() as i64;
                        let idx = if i > 0 { i - 1 } else { n + i };
                        if i == 0 || idx < 0 || idx >= n {
                            return Err("OBJ index out of range".into());
                        }
                        Ok(idx as usize)
                    })
                    .collect::<Result<_>>()?;
                if indices.len() < 3 || indices.len() > 64 {
                    return Err("OBJ face must contain 3..64 vertices".into());
                }
                for i in 1..indices.len() - 1 {
                    let v = [
                        vertices[indices[0]],
                        vertices[indices[i]],
                        vertices[indices[i + 1]],
                    ];
                    if (v[1] - v[0]).cross(v[2] - v[0]).length() > 1e-9 {
                        parts.push(triangle(v, None, m));
                    }
                }
            }
            _ => {}
        }
        if vertices.len() > 200_000 || parts.len() > 100_000 {
            return Err("OBJ exceeds vertex/triangle budget".into());
        }
    }
    if parts.is_empty() {
        return Err("OBJ contains no usable triangles".into());
    }
    Ok(parts)
}
impl Compiled {
    pub fn new(scene: Scene, base: &Path) -> Result<Self> {
        scene.validate()?;
        let mut parts = vec![];
        let mut total = 0;
        for n in &scene.nodes {
            let material = scene
                .materials
                .get(&n.material)
                .cloned()
                .unwrap_or_default();
            let p = if matches!(n.shape, Shape::Mesh) {
                load_obj(&asset_path(base, n.mesh.as_ref().unwrap())?, &material)?
            } else if matches!(n.shape, Shape::Torus) {
                torus(&material, n.tube)
            } else {
                prefab(n.shape, &material)
            };
            total += p.len() * n.repeat.count as usize;
            if total > 200_000 {
                return Err("expanded scene exceeds 200,000 primitives".into());
            }
            parts.push(Arc::new(p));
        }
        if let Some(a) = &scene.audio {
            asset_path(base, a)?;
        }
        Ok(Self { scene, parts })
    }
    pub fn at(&self, t: f32) -> World {
        let transforms = self.scene.transforms(t);
        let mut instances = Vec::new();
        for (i, parts) in self.parts.iter().enumerate() {
            let (world, visible) = transforms[i];
            if !visible {
                continue;
            }
            let motion = &self.scene.nodes[i].motion;
            let repeat = &self.scene.nodes[i].repeat;
            for copy in 0..repeat.count {
                let world = world.compose(Mat::trs(repeat.offset(copy), V::ZERO, V::ONE));
                for p in parts.iter() {
                    if p.limb == 0 {
                        instances.push(Instance::new(p, world));
                    } else {
                        let side = p.limb.signum() as f32;
                        let mut angle =
                            (t * 8. + motion.phase.to_radians()).sin() * motion.walk * side * 28.;
                        let pivot = if p.limb.abs() == 1 {
                            V(side * 0.48, 1.4, 0.)
                        } else {
                            V(side * 0.23, 0.68, 0.)
                        };
                        let mut r = V(angle, 0., 0.);
                        if p.limb == 1 && motion.wave != 0. {
                            angle = motion.wave * (125. + 25. * (t * 6.).sin());
                            r = V(0., 0., angle);
                        }
                        let joint =
                            Mat::trs(pivot, r, V::ONE).compose(Mat::trs(-pivot, V::ZERO, V::ONE));
                        instances.push(Instance::new(p, world.compose(joint)));
                    }
                }
            }
        }
        World::new(instances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_geometry_and_budget() {
        let s: Scene = serde_json::from_str(
            r#"{"nodes":[{"id":"balls","shape":"sphere","repeat":{"count":3,"step":[3,0,0]}}]}"#,
        )
        .unwrap();
        let c = Compiled::new(s, Path::new(".")).unwrap();
        let w = c.at(0.);
        assert_eq!(w.instances.len(), 3);
        assert!(w
            .hit(
                Ray {
                    o: V(6., 0., 5.),
                    d: V(0., 0., -1.)
                },
                10.,
                false
            )
            .is_some());
        let s: Scene = serde_json::from_str(
            r#"{"nodes":[{"id":"rings","shape":"torus","repeat":{"count":4096}}]}"#,
        )
        .unwrap();
        assert!(Compiled::new(s, Path::new(".")).is_err());
    }
    fn instance(shape: Primitive) -> Instance {
        Instance::new(
            &part(shape, V::ZERO, V::ONE, &Material::default()),
            Mat::identity(),
        )
    }
    #[test]
    fn analytic_intersections() {
        for p in [Primitive::Sphere, Primitive::Box, Primitive::Cylinder] {
            let h = instance(p)
                .intersect(
                    Ray {
                        o: V(0., 0., 3.),
                        d: V(0., 0., -1.),
                    },
                    100.,
                )
                .unwrap();
            assert!((h.0 - 2.).abs() < 1e-5);
            assert!(h.1 .2 > 0.99);
        }
    }
    #[test]
    fn inside_sphere_exit() {
        assert!(
            (instance(Primitive::Sphere)
                .intersect(
                    Ray {
                        o: V::ZERO,
                        d: V(1., 0., 0.)
                    },
                    100.
                )
                .unwrap()
                .0
                - 1.)
                .abs()
                < 1e-5
        );
    }
    #[test]
    fn transformed_nonuniform_sphere() {
        let i = Instance::new(
            &part(
                Primitive::Sphere,
                V::ZERO,
                V(2., 1., 0.5),
                &Material::default(),
            ),
            Mat::identity(),
        );
        assert!(
            (i.intersect(
                Ray {
                    o: V(0., 0., 3.),
                    d: V(0., 0., -1.)
                },
                10.
            )
            .unwrap()
            .0 - 2.5)
                .abs()
                < 1e-5
        );
    }
    #[test]
    fn bvh_matches_brute_force() {
        let mut v = vec![];
        for i in 0..80 {
            v.push(Instance::new(
                &part(
                    Primitive::Sphere,
                    V((i % 10) as f32 * 2., (i / 10) as f32 * 2., 0.),
                    V::ONE * 0.5,
                    &Material::default(),
                ),
                Mat::identity(),
            ));
        }
        let w = World::new(v);
        for i in 0..200 {
            let r = Ray {
                o: V(random(i) * 20., random(i + 900) * 16., 5.),
                d: V(0., 0., -1.),
            };
            let brute = w
                .instances
                .iter()
                .filter_map(|x| x.intersect(r, 100.).map(|h| h.0))
                .min_by(f32::total_cmp);
            let fast = w.hit(r, 100., false).map(|h| h.t);
            assert_eq!(brute, fast);
        }
    }
}
