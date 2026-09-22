use crate::{
    math::{Mat, V},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Track {
    Fixed(V),
    Keys(Vec<(f32, V)>),
}
impl Default for Track {
    fn default() -> Self {
        Self::Fixed(V::ZERO)
    }
}
impl Track {
    fn extrema(&self) -> (f32, f32) {
        let values: Vec<V> = match self {
            Self::Fixed(v) => vec![*v],
            Self::Keys(k) => k.iter().map(|(_, v)| *v).collect(),
        };
        values.iter().fold((f32::INFINITY, 0.0f32), |(lo, hi), v| {
            (lo.min(v.0).min(v.1).min(v.2), hi.max(v.0).max(v.1).max(v.2))
        })
    }
    pub fn at(&self, t: f32, ease: Ease) -> V {
        match self {
            Self::Fixed(v) => *v,
            Self::Keys(k) => {
                if k.is_empty() {
                    return V::ZERO;
                }
                if t <= k[0].0 {
                    return k[0].1;
                }
                for p in k.windows(2) {
                    if t <= p[1].0 {
                        let x = (t - p[0].0) / (p[1].0 - p[0].0);
                        let u = match ease {
                            Ease::Linear => x,
                            Ease::Smooth => x * x * (3. - 2. * x),
                            Ease::Smoother => x * x * x * (x * (x * 6. - 15.) + 10.),
                            Ease::Hold => 0.,
                        };
                        return p[0].1.lerp(p[1].1, u);
                    }
                }
                k.last().unwrap().1
            }
        }
    }
    fn check(&self, label: &str, scale: bool) -> Result<()> {
        let values = match self {
            Self::Fixed(v) => vec![*v],
            Self::Keys(k) => {
                if k.is_empty() || k.len() > 4096 {
                    return Err(format!("{label}: key count must be 1..4096").into());
                }
                let mut prev = -1.;
                for (t, _) in k {
                    if !t.is_finite() || *t < 0. || *t <= prev || *t > 3600. {
                        return Err(format!(
                            "{label}: key times must be finite, increasing, and within 0..3600"
                        )
                        .into());
                    }
                    prev = *t;
                }
                k.iter().map(|(_, v)| *v).collect()
            }
        };
        for v in values {
            if !v.finite() || v.0.abs() > 10000. || v.1.abs() > 10000. || v.2.abs() > 10000. {
                return Err(format!("{label}: vector must be finite and within +/-10000").into());
            }
            if scale
                && (v.0 < 0.001
                    || v.1 < 0.001
                    || v.2 < 0.001
                    || v.0 > 100.
                    || v.1 > 100.
                    || v.2 > 100.)
            {
                return Err(format!("{label}: scale components must be 0.001..100").into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    Linear,
    #[default]
    Smooth,
    Smoother,
    Hold,
}
fn one_track() -> Track {
    Track::Fixed(V::ONE)
}
fn white() -> V {
    V::ONE
}
fn rough() -> f32 {
    0.4
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Material {
    pub color: V,
    pub roughness: f32,
    pub metallic: f32,
    pub emission: f32,
    pub checker: Option<V>,
}
impl Default for Material {
    fn default() -> Self {
        Self {
            color: white(),
            roughness: rough(),
            metallic: 0.,
            emission: 0.,
            checker: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Group,
    Sphere,
    Box,
    Cylinder,
    Cone,
    Torus,
    Crystal,
    Robot,
    Tree,
    Rocket,
    Mesh,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Motion {
    pub bob: f32,
    pub frequency: f32,
    pub phase: f32,
    pub spin: V,
    pub orbit: f32,
    pub orbit_speed: f32,
    pub walk: f32,
    pub wave: f32,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            bob: 0.,
            frequency: 1.,
            phase: 0.,
            spin: V::ZERO,
            orbit: 0.,
            orbit_speed: 30.,
            walk: 0.,
            wave: 0.,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Repeat {
    pub count: u32,
    pub step: V,
    pub ring: f32,
    pub jitter: V,
    pub seed: u32,
}
impl Default for Repeat {
    fn default() -> Self {
        Self {
            count: 1,
            step: V::ZERO,
            ring: 0.,
            jitter: V::ZERO,
            seed: 1,
        }
    }
}
impl Repeat {
    pub fn offset(&self, i: u32) -> V {
        use crate::math::random;
        let angle = i as f32 / self.count as f32 * std::f32::consts::TAU;
        let seed = self.seed.wrapping_add(i.wrapping_mul(997));
        let jitter = V(
            random(seed),
            random(seed.wrapping_add(19)),
            random(seed.wrapping_add(37)),
        ) * 2.
            - V::ONE;
        self.step * i as f32 + V(angle.cos(), 0., angle.sin()) * self.ring + jitter * self.jitter
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub shape: Shape,
    pub material: String,
    pub parent: Option<String>,
    pub pos: Track,
    pub rot: Track,
    pub scale: Track,
    pub ease: Ease,
    pub motion: Motion,
    pub visible: Option<[f32; 2]>,
    pub mesh: Option<String>,
    pub tube: f32,
    pub repeat: Repeat,
}
impl Default for Node {
    fn default() -> Self {
        Self {
            id: String::new(),
            shape: Shape::Group,
            material: "default".into(),
            parent: None,
            pos: Track::default(),
            rot: Track::default(),
            scale: one_track(),
            ease: Ease::default(),
            motion: Motion::default(),
            visible: None,
            mesh: None,
            tube: 0.22,
            repeat: Repeat::default(),
        }
    }
}
impl Node {
    pub fn local(&self, t: f32) -> Mat {
        let mut p = self.pos.at(t, self.ease);
        let mut r = self.rot.at(t, self.ease);
        let m = &self.motion;
        p.1 += m.bob * (t * m.frequency * std::f32::consts::TAU + m.phase.to_radians()).sin();
        r = r + m.spin * t;
        if m.orbit != 0. {
            let a = (t * m.orbit_speed + m.phase).to_radians();
            p = p + V(a.cos() * m.orbit, 0., a.sin() * m.orbit);
        }
        Mat::trs(p, r, self.scale.at(t, self.ease))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Camera {
    pub pos: Track,
    pub target: Track,
    pub fov: f32,
    pub orbit: f32,
    pub aperture: f32,
    pub focus: f32,
    pub ease: Ease,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            pos: Track::Fixed(V(7., 4.5, 9.)),
            target: Track::Fixed(V(0., 1., 0.)),
            fov: 42.,
            orbit: 0.,
            aperture: 0.,
            focus: 10.,
            ease: Ease::Smooth,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Light {
    pub pos: Track,
    pub color: V,
    pub power: f32,
    pub radius: f32,
}
impl Default for Light {
    fn default() -> Self {
        Self {
            pos: Track::Fixed(V(-4., 8., 5.)),
            color: V(1., 0.88, 0.72),
            power: 90.,
            radius: 1.2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct World {
    pub sky: V,
    pub horizon: V,
    pub ambient: f32,
    pub fog: f32,
    pub exposure: f32,
    pub bloom: f32,
}
impl Default for World {
    fn default() -> Self {
        Self {
            sky: V(0.025, 0.045, 0.09),
            horizon: V(0.16, 0.22, 0.28),
            ambient: 0.24,
            fog: 0.012,
            exposure: 1.15,
            bloom: 0.22,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scene {
    pub version: u32,
    pub size: [u32; 2],
    pub fps: u32,
    pub duration: f32,
    pub camera: Camera,
    pub world: World,
    pub materials: BTreeMap<String, Material>,
    pub lights: Vec<Light>,
    pub nodes: Vec<Node>,
    pub audio: Option<String>,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            version: 1,
            size: [1280, 720],
            fps: 30,
            duration: 6.,
            camera: Camera::default(),
            world: World::default(),
            materials: BTreeMap::new(),
            lights: vec![Light::default()],
            nodes: vec![],
            audio: None,
        }
    }
}
fn range(x: f32, a: f32, b: f32, label: &str) -> Result<()> {
    if !x.is_finite() || x < a || x > b {
        Err(format!("{label}: expected {a}..{b}, got {x}").into())
    } else {
        Ok(())
    }
}
fn color(v: V, label: &str) -> Result<()> {
    for x in [v.0, v.1, v.2] {
        range(x, 0., 1., label)?;
    }
    Ok(())
}
pub fn asset_path(base: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let p = Path::new(relative);
    if p.is_absolute()
        || p.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err("assets must use relative paths inside the scene directory".into());
    }
    let root = base.canonicalize()?;
    let target = root.join(p).canonicalize()?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err("asset is not a regular file inside the scene directory".into());
    }
    Ok(target)
}
impl Scene {
    pub fn frames(&self) -> u32 {
        // Decimal scene durations often lie a fraction above an exact frame boundary
        // after f32 conversion. Snap within representation error before rounding up.
        let raw = self.duration as f64 * self.fps as f64;
        let rounded = raw.round();
        let tolerance = raw.abs().max(1.) * f32::EPSILON as f64;
        (if (raw - rounded).abs() <= tolerance {
            rounded
        } else {
            raw.ceil()
        })
        .max(1.) as u32
    }
    pub fn load(path: &Path) -> Result<Self> {
        if std::fs::metadata(path)?.len() > 8 * 1024 * 1024 {
            return Err("scene exceeds 8 MiB limit".into());
        }
        let s: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        s.validate()?;
        Ok(s)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err("unsupported scene version; expected 1".into());
        }
        if self.size.iter().any(|x| *x < 16 || *x > 3840 || x % 2 != 0)
            || self.size[0] as u64 * self.size[1] as u64 > 8_294_400
        {
            return Err("size: even dimensions, 16..3840, at most 8,294,400 pixels".into());
        }
        if self.fps < 1 || self.fps > 120 {
            return Err("fps must be 1..120".into());
        }
        range(self.duration, 0.01, 3600., "duration")?;
        if self.nodes.len() > 4096 || self.lights.len() > 16 || self.materials.len() > 1024 {
            return Err("scene exceeds node/light/material limits (4096/16/1024)".into());
        }
        self.camera.pos.check("camera.pos", false)?;
        self.camera.target.check("camera.target", false)?;
        range(self.camera.fov, 5., 150., "camera.fov")?;
        range(self.camera.orbit, -3600., 3600., "camera.orbit")?;
        range(self.camera.aperture, 0., 1., "camera.aperture")?;
        range(self.camera.focus, 0.01, 10000., "camera.focus")?;
        color(self.world.sky, "world.sky")?;
        color(self.world.horizon, "world.horizon")?;
        range(self.world.ambient, 0., 4., "world.ambient")?;
        range(self.world.fog, 0., 1., "world.fog")?;
        range(self.world.exposure, 0.01, 10., "world.exposure")?;
        range(self.world.bloom, 0., 2., "world.bloom")?;
        for (name, m) in &self.materials {
            color(m.color, &format!("material {name}.color"))?;
            if let Some(c) = m.checker {
                color(c, "checker")?;
            }
            range(m.roughness, 0.04, 1., "roughness")?;
            range(m.metallic, 0., 1., "metallic")?;
            range(m.emission, 0., 30., "emission")?;
        }
        for l in &self.lights {
            l.pos.check("light.pos", false)?;
            color(l.color, "light.color")?;
            range(l.power, 0., 10000., "light.power")?;
            range(l.radius, 0., 100., "light.radius")?;
        }
        let mut ids = HashSet::new();
        for n in &self.nodes {
            if n.id.is_empty() || n.id.len() > 128 || !ids.insert(n.id.as_str()) {
                return Err(format!("node id must be nonempty and unique: {:?}", n.id).into());
            }
            if n.material != "default" && !self.materials.contains_key(&n.material) {
                return Err(format!("node {}: unknown material {}", n.id, n.material).into());
            }
            range(n.tube, 0.005, 0.45, "tube")?;
            if !(1..=4096).contains(&n.repeat.count) {
                return Err("repeat.count must be 1..4096".into());
            }
            range(n.repeat.ring, 0., 1000., "repeat.ring")?;
            Track::Fixed(n.repeat.step).check("repeat.step", false)?;
            Track::Fixed(n.repeat.jitter).check("repeat.jitter", false)?;
            n.pos.check(&format!("{}.pos", n.id), false)?;
            n.rot.check(&format!("{}.rot", n.id), false)?;
            n.scale.check(&format!("{}.scale", n.id), true)?;
            let m = &n.motion;
            for (label, x) in [
                ("bob", m.bob),
                ("frequency", m.frequency),
                ("phase", m.phase),
                ("orbit", m.orbit),
                ("orbit_speed", m.orbit_speed),
                ("walk", m.walk),
                ("wave", m.wave),
            ] {
                range(x, -1000., 1000., label)?;
            }
            Track::Fixed(m.spin).check("motion.spin", false)?;
            if let Some([a, b]) = n.visible {
                range(a, 0., self.duration, "visible start")?;
                range(b, a, self.duration, "visible end")?;
            }
            if matches!(n.shape, Shape::Mesh) && n.mesh.is_none() {
                return Err(format!("{}: mesh path required", n.id).into());
            }
        }
        for n in &self.nodes {
            let mut seen = HashSet::new();
            let mut at = n;
            let (mut min_scale, mut max_scale) = at.scale.extrema();
            while let Some(p) = &at.parent {
                if !seen.insert(at.id.as_str()) {
                    return Err(format!("parent cycle involving {}", n.id).into());
                }
                if seen.len() > 16 {
                    return Err("parent hierarchy exceeds 16 levels".into());
                }
                at = self
                    .nodes
                    .iter()
                    .find(|x| x.id == *p)
                    .ok_or_else(|| format!("{}: missing parent {p}", n.id))?;
                let (lo, hi) = at.scale.extrema();
                min_scale *= lo;
                max_scale *= hi;
                if min_scale < 0.00001 || max_scale > 10000. {
                    return Err(format!(
                        "{}: combined parent scales exceed numerical stability limits",
                        n.id
                    )
                    .into());
                }
            }
        }
        Ok(())
    }
    pub fn transforms(&self, t: f32) -> Vec<(Mat, bool)> {
        fn get(s: &Scene, i: usize, t: f32, cache: &mut [Option<(Mat, bool)>]) -> (Mat, bool) {
            if let Some(v) = cache[i] {
                return v;
            }
            let n = &s.nodes[i];
            let mut m = n.local(t);
            let mut visible = n.visible.is_none_or(|[a, b]| t >= a && t <= b);
            if let Some(p) = &n.parent {
                let j = s.nodes.iter().position(|x| x.id == *p).unwrap();
                let (pm, pv) = get(s, j, t, cache);
                m = pm.compose(m);
                visible &= pv;
            }
            cache[i] = Some((m, visible));
            (m, visible)
        }
        let mut cache = vec![None; self.nodes.len()];
        (0..self.nodes.len())
            .map(|i| get(self, i, t, &mut cache))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_unknown_fields() {
        assert!(serde_json::from_str::<Scene>(r#"{"duraton":4}"#).is_err());
    }
    #[test]
    fn keyframes_clamp_and_smooth() {
        let t = Track::Keys(vec![(1., V::ZERO), (3., V::ONE)]);
        assert_eq!(t.at(0., Ease::Smooth), V::ZERO);
        assert_eq!(t.at(2., Ease::Smooth), V::ONE * 0.5);
        assert_eq!(t.at(9., Ease::Smooth), V::ONE);
    }
    #[test]
    fn rejects_duplicate_keys() {
        let t = Track::Keys(vec![(0., V::ZERO), (0., V::ONE)]);
        assert!(t.check("x", false).is_err());
    }
    #[test]
    fn rejects_cycles() {
        let s: Scene =
            serde_json::from_str(r#"{"nodes":[{"id":"a","parent":"b"},{"id":"b","parent":"a"}]}"#)
                .unwrap();
        assert!(s.validate().unwrap_err().to_string().contains("cycle"));
    }
    #[test]
    fn rejects_singular_scale() {
        let s: Scene = serde_json::from_str(r#"{"nodes":[{"id":"a","scale":[1,0,1]}]}"#).unwrap();
        assert!(s.validate().is_err());
    }
    #[test]
    fn decimal_frame_count() {
        let mut s = Scene {
            duration: 0.1,
            ..Scene::default()
        };
        assert_eq!(s.frames(), 3);
        s.duration = 0.101;
        assert_eq!(s.frames(), 4);
    }
    #[test]
    fn rejects_explosive_parent_scales() {
        let s: Scene = serde_json::from_str(r#"{"nodes":[{"id":"a","scale":[100,100,100]},{"id":"b","parent":"a","scale":[100,100,100]},{"id":"c","parent":"b","scale":[100,100,100]}]}"#).unwrap();
        assert!(s.validate().unwrap_err().to_string().contains("stability"));
    }
    #[test]
    fn hierarchy_and_visibility() {
        let s:Scene=serde_json::from_str(r#"{"nodes":[{"id":"a","pos":[1,0,0],"visible":[0,1]},{"id":"b","parent":"a","pos":[0,2,0]}]}"#).unwrap();
        s.validate().unwrap();
        let t = s.transforms(2.);
        assert_eq!(t[1].0.p, V(1., 2., 0.));
        assert!(!t[1].1);
    }
}
