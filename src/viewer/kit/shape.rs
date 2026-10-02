//! Procedural modelling beyond boxes and balls: lofts, sweeps, rounded boxes, capsules, mirrored
//! halves, smooth normals and curvature-safe ribbons.
//!
//! Most of this is lifted from Deadfall's weapon-model toolkit (33 hand-modelled weapons, where art cost
//! two thirds of the whole project) and from Spooky Kart's road border, so the next game does not rewrite
//! it. Everything adds to a [`Template`] and follows the template's conventions:
//!
//! * Units are whatever the template uses (the examples are metres). Epsilons here are in those units.
//! * Triangles are counter-clockwise seen from the front, and the front is where the vertex normals point.
//!   Every method here picks the winding from the normals, so a surface can never come out inside-out;
//!   [`lint`](super::lint) checks the same agreement for meshes built any other way.
//! * Positive `lateral` offsets (strips) are to the **right** of the direction of travel seen from above
//!   (+Y towards the viewer, so travelling towards -Z has +X on the right).
//! * A template holds at most 65 535 vertices; these methods panic past that exactly like the primitives.
//!
//! | Method | Job |
//! |---|---|
//! | [`Template::loft`] | skin a list of [`Section`]s (rounded rectangles or ellipses) into a smooth solid |
//! | [`Template::sweep`] | a tube or bar following a path ([`Sweep`] says how it is shaped) |
//! | [`Template::rounded_box`], [`Template::capsule`], [`Template::rod`] | the common solids |
//! | [`Template::mirror_x`], [`Template::mirrored_x`] | model half a thing, get both halves with correct winding |
//! | [`Template::smooth_normals`] | weld by position and average normals under an angle threshold |
//! | [`Template::offset_strip`], [`Template::offset_band`], [`strip_quads`], [`offset_path`] | roads, kerbs, walls' footprints: a ribbon along a polyline that cannot fold over itself |
//! | [`quad_bezier`], [`arc`] | path points for [`Template::sweep`] |
use super::batch::{Rgb, Template, Vert};
use macroquad::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Four corners of one strip segment: `[a_i, b_i, b_j, a_j]`, where `a` is the edge at the `from` lateral
/// offset and `b` the one at `to`, and `i -> j` is the direction of travel. See [`strip_quads`].
pub type Quad = [Vec3; 4];

// ---------------------------------------------------------------------------------------------------------
// Lofts
// ---------------------------------------------------------------------------------------------------------

/// The cross-section shape every [`Section`] of one loft is cut to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    /// Rounded rectangle with this many segments per rounded corner (0 = sharp corners). A section whose
    /// corner radii are both zero is a plain rectangle.
    Rounded(usize),
    /// Ellipse with this many sides (at least 3).
    Ellipse(usize),
}

/// One cross-section of a [`Template::loft`]: a rounded rectangle (or ellipse) of half sizes `half_w`
/// along `ex` and `half_h` along `ey`, centred on `center`. A section of size zero is a point (the tip of
/// a bullet or the pole of a ball).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    /// Centre of the section.
    pub center: Vec3,
    /// Unit direction of the width axis.
    pub ex: Vec3,
    /// Unit direction of the height axis. `ex.cross(ey)` is the direction the loft is being laid along,
    /// give or take a sign: the winding is taken from the normals, not from this.
    pub ey: Vec3,
    /// Half size along `ex`.
    pub half_w: f32,
    /// Half size along `ey`.
    pub half_h: f32,
    /// Corner radius of the two corners on the `+ey` side (only for [`Ring::Rounded`]).
    pub round_top: f32,
    /// Corner radius of the two corners on the `-ey` side (only for [`Ring::Rounded`]).
    pub round_bottom: f32,
}

impl Section {
    /// A square-cornered section.
    pub fn new(center: Vec3, ex: Vec3, ey: Vec3, half_w: f32, half_h: f32) -> Self {
        Self {
            center,
            ex,
            ey,
            half_w,
            half_h,
            round_top: 0.,
            round_bottom: 0.,
        }
    }
    /// The same section with these corner radii (clamped to the smaller half size when cut).
    pub fn with_corners(mut self, top: f32, bottom: f32) -> Self {
        self.round_top = top;
        self.round_bottom = bottom;
        self
    }
    /// A section standing across the Z axis at depth `z`, centred on (`x`, `y`), width along X and height
    /// along Y, all four corners rounded by `radius`. A loft through such sections runs along Z.
    pub fn across_z(z: f32, x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> Self {
        Self::new(vec3(x, y, z), Vec3::X, Vec3::Y, half_w, half_h).with_corners(radius, radius)
    }
}

fn ring_points(s: &Section, ring: Ring, soft_top: bool, soft_bottom: bool) -> Vec<(Vec3, Vec3)> {
    let at = |x: f32, y: f32, nx: f32, ny: f32| {
        (
            s.center + s.ex * x + s.ey * y,
            (s.ex * nx + s.ey * ny).normalize_or_zero(),
        )
    };
    let mut out = Vec::new();
    match ring {
        Ring::Ellipse(n) => {
            let n = n.max(3);
            for i in 0..n {
                let (sn, cs) = (TAU * i as f32 / n as f32).sin_cos();
                out.push(at(
                    s.half_w * cs,
                    s.half_h * sn,
                    cs / s.half_w.max(1e-5),
                    sn / s.half_h.max(1e-5),
                ));
            }
        }
        Ring::Rounded(segs) => {
            let lim = s.half_w.min(s.half_h);
            let (rt, rb) = (s.round_top.clamp(0., lim), s.round_bottom.clamp(0., lim));
            // Corner centre, radius, start angle, rounded?
            let corners = [
                (s.half_w - rt, s.half_h - rt, rt, 0., soft_top),
                (-(s.half_w - rt), s.half_h - rt, rt, FRAC_PI_2, soft_top),
                (-(s.half_w - rb), -(s.half_h - rb), rb, PI, soft_bottom),
                (
                    s.half_w - rb,
                    -(s.half_h - rb),
                    rb,
                    PI + FRAC_PI_2,
                    soft_bottom,
                ),
            ];
            for (cx, cy, r, a0, soft) in corners {
                let k = if soft { segs.max(1) + 1 } else { 2 };
                for i in 0..k {
                    let (sn, cs) = (a0 + FRAC_PI_2 * i as f32 / (k - 1) as f32).sin_cos();
                    out.push(at(cx + r * cs, cy + r * sn, cs, sn));
                }
            }
        }
    }
    out
}

/// A unit vector perpendicular to `u` and a second one perpendicular to both.
fn basis_perp(u: Vec3) -> (Vec3, Vec3) {
    let helper = if u.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let ex = u.cross(helper).normalize_or_zero();
    (ex, u.cross(ex))
}

impl Template {
    /// One triangle wound to face the way its vertex normals point; collapsed triangles are dropped.
    fn tri_facing(&mut self, a: u16, b: u16, c: u16) {
        let (va, vb, vc) = (
            self.verts[a as usize],
            self.verts[b as usize],
            self.verts[c as usize],
        );
        let g = (vb.p - va.p).cross(vc.p - va.p);
        if g.length_squared() < 1e-14 {
            return;
        }
        if g.dot(va.n + vb.n + vc.n) >= 0. {
            self.idx.extend_from_slice(&[a, b, c]);
        } else {
            self.idx.extend_from_slice(&[a, c, b]);
        }
    }

    /// Skin sections into a smooth solid. `closed` joins the last section back to the first (a torus).
    #[allow(clippy::too_many_arguments)]
    fn skin(&mut self, secs: &[Section], ring: Ring, caps: bool, closed: bool, c: Rgb, e: f32) {
        let k = secs.len();
        if k < 2 || (closed && k < 3) {
            return;
        }
        let soft_top = secs.iter().any(|s| s.round_top > 1e-5);
        let soft_bottom = secs.iter().any(|s| s.round_bottom > 1e-5);
        let rings: Vec<Vec<(Vec3, Vec3)>> = secs
            .iter()
            .map(|s| ring_points(s, ring, soft_top, soft_bottom))
            .collect();
        let m = rings[0].len();
        let base = self.verts.len();
        for i in 0..k {
            let prev = if i > 0 {
                i - 1
            } else if closed {
                k - 1
            } else {
                0
            };
            let next = if i + 1 < k {
                i + 1
            } else if closed {
                0
            } else {
                k - 1
            };
            for j in 0..m {
                let (p, np) = rings[i][j];
                // The ring's own normal, made perpendicular to the way the surface runs along the loft.
                let d = (rings[next][j].0 - rings[prev][j].0).normalize_or_zero();
                let n = (np - d * np.dot(d)).normalize_or_zero();
                self.push(p, if n == Vec3::ZERO { np } else { n }, c, e);
            }
        }
        let at = |i: usize, j: usize| (base + i * m + j) as u16;
        let bands = if closed { k } else { k - 1 };
        for i in 0..bands {
            let i2 = (i + 1) % k;
            for j in 0..m {
                let j2 = (j + 1) % m;
                self.tri_facing(at(i, j), at(i, j2), at(i2, j2));
                self.tri_facing(at(i, j), at(i2, j2), at(i2, j));
            }
        }
        if caps && !closed {
            for (i, nb) in [(0usize, 1usize), (k - 1, k - 2)] {
                let s = &secs[i];
                if s.half_w < 1e-4 && s.half_h < 1e-4 {
                    continue; // a point: nothing to close
                }
                let mut nrm = (s.center - secs[nb].center).normalize_or_zero();
                if nrm == Vec3::ZERO {
                    nrm = s.ex.cross(s.ey).normalize_or_zero();
                }
                let hub = self.push(s.center, nrm, c, e);
                let first = self.verts.len();
                for (p, _) in &rings[i] {
                    self.push(*p, nrm, c, e);
                }
                for j in 0..m {
                    self.tri_facing(hub, (first + j) as u16, (first + (j + 1) % m) as u16);
                }
            }
        }
    }

    /// Skin a list of sections into a smooth, capped solid (a gun body, a limb, a bottle).
    ///
    /// Consecutive sections are joined by bands of quads and the first and last are closed with a flat
    /// cap (skipped for a section of size zero). Normals are smooth along the ring and follow the
    /// surface along the loft. All sections must be cut with the same `ring`; whether corners are rounded
    /// is decided for the whole loft (a corner radius above zero on any section rounds that corner on all,
    /// the others keep radius zero). Fewer than two sections add nothing.
    pub fn loft(&mut self, sections: &[Section], ring: Ring, caps: bool, c: Rgb, e: f32) {
        self.skin(sections, ring, caps, false, c, e);
    }

    /// A bar or tube following `path`, shaped by `profile`. See [`Sweep`] for the frame it uses.
    ///
    /// Consecutive duplicate points are ignored. A path whose last point equals its first (and has more
    /// than three points) becomes a closed ring (a torus-like loop) with no caps; an open path is capped.
    pub fn sweep(&mut self, path: &[Vec3], profile: &Sweep, c: Rgb, e: f32) {
        let mut pts: Vec<Vec3> = Vec::with_capacity(path.len());
        for p in path {
            if pts.last().is_none_or(|q| (*q - *p).length() > 1e-6) {
                pts.push(*p);
            }
        }
        let closed = pts.len() > 3 && (pts[0] - pts[pts.len() - 1]).length() < 1e-5;
        if closed {
            pts.pop();
        }
        let k = pts.len();
        if k < 2 {
            return;
        }
        let ex = {
            let s = profile.side.normalize_or_zero();
            if s == Vec3::ZERO {
                Vec3::X
            } else {
                s
            }
        };
        let mut last_ey = ex.any_orthonormal_vector();
        let secs: Vec<Section> = (0..k)
            .map(|i| {
                let (prev, next) = if closed {
                    ((i + k - 1) % k, (i + 1) % k)
                } else {
                    (i.saturating_sub(1), (i + 1).min(k - 1))
                };
                let ey = (pts[next] - pts[prev]).cross(ex).normalize_or_zero();
                let ey = if ey == Vec3::ZERO { last_ey } else { ey };
                last_ey = ey;
                let t = i as f32 / (k - 1) as f32;
                let lerp = |a: [f32; 2]| a[0] + (a[1] - a[0]) * t;
                let (w, h) = (lerp(profile.half_width), lerp(profile.half_height));
                let corner = match profile.ring {
                    Ring::Rounded(_) => w.min(h) * 0.4,
                    Ring::Ellipse(_) => 0.,
                };
                Section::new(pts[i], ex, ey, w, h).with_corners(corner, corner)
            })
            .collect();
        self.skin(&secs, profile.ring, !closed, closed, c, e);
    }

    /// A box with every edge rounded by `radius` (clamped to half the smallest dimension), smooth shaded.
    /// `half` is the half extent per axis, like [`Template::box_`]; the outer size is exactly
    /// `center +- half`.
    pub fn rounded_box(&mut self, center: Vec3, half: Vec3, radius: f32, c: Rgb, e: f32) {
        const STEPS: usize = 3;
        let r = radius.min(half.x).min(half.y).min(half.z).max(1e-4);
        let section = |z: f32, inset: f32, rad: f32| {
            Section::across_z(
                z,
                center.x,
                center.y,
                (half.x - inset).max(3e-4),
                (half.y - inset).max(3e-4),
                rad,
            )
        };
        // A quarter circle in STEPS steps rounds the near end, then the same mirrored rounds the far end.
        let curve = |i: usize| {
            let th = FRAC_PI_2 * i as f32 / STEPS as f32;
            (r * (1. - th.cos()), r * (1. - th.sin()), r * th.sin())
        };
        let mut secs = Vec::with_capacity(2 * (STEPS + 1));
        for i in 0..=STEPS {
            let (dz, inset, rad) = curve(i);
            secs.push(section(center.z - half.z + dz, inset, rad));
        }
        for i in (0..=STEPS).rev() {
            let (dz, inset, rad) = curve(i);
            secs.push(section(center.z + half.z - dz, inset, rad));
        }
        self.loft(&secs, Ring::Rounded(STEPS), true, c, e);
    }

    /// A capsule (a cylinder with hemispherical ends): bones, limbs, barrels, bullets, pills. `a` and
    /// `b` are the centres of the two end hemispheres, so the total length is `|b - a| + 2 r`. `sides`
    /// is the number of facets round the axis (at least 3).
    pub fn capsule(&mut self, a: Vec3, b: Vec3, r: f32, c: Rgb, e: f32, sides: usize) {
        let sides = sides.max(3);
        let hs = (sides / 4).max(2);
        let axis = b - a;
        let u = if axis.length() > 1e-6 {
            axis.normalize()
        } else {
            Vec3::Y
        };
        let (ex, ey) = basis_perp(u);
        // Stations along the axis: (centre, latitude). The two latitude-0 stations bound the cylinder.
        let mut stations = Vec::with_capacity(2 * (hs + 1));
        for i in 0..=hs {
            stations.push((a, -FRAC_PI_2 + FRAC_PI_2 * i as f32 / hs as f32));
        }
        for i in 0..=hs {
            stations.push((b, FRAC_PI_2 * i as f32 / hs as f32));
        }
        let base = self.verts.len();
        for (centre, lat) in &stations {
            let (sl, cl) = lat.sin_cos();
            for j in 0..=sides {
                let (sa, ca) = (TAU * j as f32 / sides as f32).sin_cos();
                let n = u * sl + (ex * ca + ey * sa) * cl;
                self.push(*centre + n * r, n, c, e);
            }
        }
        let w = sides + 1;
        for i in 0..stations.len() - 1 {
            for j in 0..sides {
                let q = |di: usize, dj: usize| (base + (i + di) * w + j + dj) as u16;
                self.tri_facing(q(0, 0), q(0, 1), q(1, 1));
                self.tri_facing(q(0, 0), q(1, 1), q(1, 0));
            }
        }
    }

    /// A capped cone or cylinder between two arbitrary points: radius `r0` at `a`, `r1` at `b`. Where
    /// [`Template::cone`] always stands on +Y, this is the "stick from here to there" a rig of limbs,
    /// barrels and struts needs. Nothing is added when the points coincide.
    #[allow(clippy::too_many_arguments)]
    pub fn rod(&mut self, a: Vec3, b: Vec3, r0: f32, r1: f32, c: Rgb, e: f32, sides: usize) {
        let d = b - a;
        let len = d.length();
        if len < 1e-6 {
            return;
        }
        let mut t = Template {
            alpha: self.alpha,
            ..Template::default()
        };
        t.cone(Vec3::ZERO, r0, r1, len, c, e, sides.max(3));
        let m =
            Mat4::from_translation(a) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, d / len));
        self.append(&t.transformed(m));
    }

    /// This template reflected across the YZ plane (x becomes -x): positions and normals are mirrored and
    /// every triangle's winding is reversed, which a plain `x = -x` would leave inside-out.
    pub fn mirrored_x(&self) -> Template {
        let mut m = self.clone();
        for v in &mut m.verts {
            v.p.x = -v.p.x;
            v.n.x = -v.n.x;
        }
        for tri in m.idx.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
        m
    }

    /// Add the mirror image of everything built so far: model the right half (x >= 0), call this, and
    /// get a symmetric whole (a face, a vehicle, a weapon). The vertices on the x = 0 plane are
    /// duplicated, not welded, so the seam shades flat unless [`Template::smooth_normals`] runs after.
    pub fn mirror_x(&mut self) {
        let m = self.mirrored_x();
        self.append(&m);
    }

    /// Rebuild the vertices with smooth normals: vertices at the same position (within 0.0001 units) are
    /// welded, and each corner of each triangle gets the angle-weighted average of the normals of every
    /// triangle meeting there whose normal is within `max_angle_deg` of its own, so soft curves blend and
    /// real edges stay hard. Around 30-60 degrees is typical. Needs correct winding (it takes face normals
    /// from it) and drops triangles with an index out of range. Corners that end up identical share one
    /// vertex, so a faceted mesh built from loose quads gets smaller.
    pub fn smooth_normals(&mut self, max_angle_deg: f32) {
        let cos_max = max_angle_deg.to_radians().cos();
        let cell = |p: Vec3| {
            [
                (p.x / 1e-4).round() as i64,
                (p.y / 1e-4).round() as i64,
                (p.z / 1e-4).round() as i64,
            ]
        };
        let corner_angle = |a: Vec3, b: Vec3, c: Vec3| {
            let (u, v) = (b - a, c - a);
            u.cross(v).length().atan2(u.dot(v))
        };
        let tris: Vec<[usize; 3]> = self
            .idx
            .chunks_exact(3)
            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
            .filter(|t| t.iter().all(|&i| i < self.verts.len()))
            .collect();
        // Face normal per triangle (None when collapsed), and the faces meeting at each position.
        let mut faces: Vec<Option<Vec3>> = Vec::with_capacity(tris.len());
        let mut at_point: HashMap<[i64; 3], Vec<(Vec3, f32)>> = HashMap::new();
        for t in &tris {
            let p = [self.verts[t[0]].p, self.verts[t[1]].p, self.verts[t[2]].p];
            let g = (p[1] - p[0]).cross(p[2] - p[0]);
            if g.length_squared() < 1e-14 {
                faces.push(None);
                continue;
            }
            let n = g.normalize();
            faces.push(Some(n));
            for k in 0..3 {
                let w = corner_angle(p[k], p[(k + 1) % 3], p[(k + 2) % 3]);
                at_point.entry(cell(p[k])).or_default().push((n, w));
            }
        }
        let mut verts: Vec<Vert> = Vec::new();
        let mut idx: Vec<u16> = Vec::with_capacity(tris.len() * 3);
        let mut seen: HashMap<([i64; 3], [i32; 3], [u32; 5]), u16> = HashMap::new();
        for (t, face) in tris.iter().zip(&faces) {
            for &i in t {
                let v = self.verts[i];
                let n = match face {
                    Some(fnorm) => {
                        let sum = at_point[&cell(v.p)]
                            .iter()
                            .filter(|(gn, _)| gn.dot(*fnorm) >= cos_max)
                            .fold(Vec3::ZERO, |s, (gn, w)| s + *gn * *w);
                        let n = sum.normalize_or_zero();
                        if n == Vec3::ZERO {
                            *fnorm
                        } else {
                            n
                        }
                    }
                    None => v.n,
                };
                let key = (
                    cell(v.p),
                    [
                        (n.x * 1000.).round() as i32,
                        (n.y * 1000.).round() as i32,
                        (n.z * 1000.).round() as i32,
                    ],
                    [
                        v.c[0].to_bits(),
                        v.c[1].to_bits(),
                        v.c[2].to_bits(),
                        v.e.to_bits(),
                        v.a.to_bits(),
                    ],
                );
                let new = *seen.entry(key).or_insert_with(|| {
                    verts.push(Vert { n, ..v });
                    u16::try_from(verts.len() - 1).expect(
                        "a Template holds at most 65 535 vertices: build big scenes from several templates",
                    )
                });
                idx.push(new);
            }
        }
        self.verts = verts;
        self.idx = idx;
    }
}

/// How a [`Template::sweep`] shapes its section along the path.
///
/// The section is a rounded rectangle or ellipse whose width axis is the fixed `side` direction and whose
/// height axis lies across the path and `side` (`tangent x side`). A fixed side axis is exactly right for
/// a **planar** path: set `side` to the plane's normal (a path in the XY plane has `side = Vec3::Z`). A
/// twisting 3D path is not supported (the frame would flip where the path turns parallel to `side`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    /// The plane normal of a planar path: the section's width direction. Must not be zero.
    pub side: Vec3,
    /// Half size along `side` at the first and the last path point (linear in between).
    pub half_width: [f32; 2],
    /// Half size across the path, within its plane, at the first and the last point.
    pub half_height: [f32; 2],
    /// [`Ring::Ellipse`] for a round tube, [`Ring::Rounded`] for a bar with corners rounded by 40% of
    /// the smaller half size.
    pub ring: Ring,
}

impl Sweep {
    /// A round wire or tube of constant `radius` (8 sides) whose path lies in the plane with normal `side`.
    pub fn round(side: Vec3, radius: f32) -> Self {
        Self {
            side,
            half_width: [radius; 2],
            half_height: [radius; 2],
            ring: Ring::Ellipse(8),
        }
    }
    /// A flat strap `half_width` wide along `side` and `half_thickness` thick across the path.
    pub fn strap(side: Vec3, half_width: f32, half_thickness: f32) -> Self {
        Self {
            side,
            half_width: [half_width; 2],
            half_height: [half_thickness; 2],
            ring: Ring::Rounded(1),
        }
    }
}

/// Points along the quadratic Bezier curve from `p0` to `p2` bent towards `p1`: `n` segments, `n + 1`
/// points (a curved magazine, a handle, a pipe run). Pass the result to [`Template::sweep`].
pub fn quad_bezier(p0: Vec3, p1: Vec3, p2: Vec3, n: usize) -> Vec<Vec3> {
    let n = n.max(1);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            p0 * ((1. - t) * (1. - t)) + p1 * (2. * t * (1. - t)) + p2 * (t * t)
        })
        .collect()
}

/// Points on an elliptic arc: `center + u * (r[0] cos a) + v * (r[1] sin a)` for `a` from `a0` to `a1`
/// (radians) in `n` segments (`n + 1` points). `u` and `v` are unit directions spanning the arc's plane;
/// a full circle is `a0 = 0, a1 = TAU`, whose last point equals its first, which [`Template::sweep`] reads
/// as a closed loop.
pub fn arc(center: Vec3, u: Vec3, v: Vec3, r: [f32; 2], a0: f32, a1: f32, n: usize) -> Vec<Vec3> {
    let n = n.max(1);
    (0..=n)
        .map(|i| {
            let (s, c) = (a0 + (a1 - a0) * i as f32 / n as f32).sin_cos();
            center + u * (r[0] * c) + v * (r[1] * s)
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------
// Strips
// ---------------------------------------------------------------------------------------------------------

/// How [`offset_path`], [`strip_quads`] and [`Template::offset_strip`] treat corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripOpts {
    /// Longest a mitred corner may stretch the offset, as a multiple of the offset (1 = never stretch).
    /// Square corners need 1.42; a hairpin would need infinity, so the default 2 caps the spike and the
    /// strip narrows there instead. Values below 1 count as 1.
    pub miter_limit: f32,
    /// On the inside of a bend the offset is kept short of the local centre of curvature: it never
    /// exceeds this fraction of the bend's radius (see [`offset_path`]). 0.95 is the default and any
    /// value below 1 cannot fold; 0 turns the clamp off (and brings the folds back).
    pub clamp_to_curvature: f32,
    /// Added to every y: lifts a strip clear of the surface it lies on. Coplanar surfaces z-fight, and a
    /// few millimetres is not enough at a distance: see [`depth_resolution`](super::lint::depth_resolution).
    pub lift: f32,
}

impl Default for StripOpts {
    fn default() -> Self {
        Self {
            miter_limit: 2.,
            clamp_to_curvature: 0.95,
            lift: 0.,
        }
    }
}

fn right_of(t: Vec2) -> Vec2 {
    vec2(-t.y, t.x)
}

/// Remove consecutive points that coincide in XZ (and, for a closed path, a last point equal to the first).
fn dedupe(path: &[Vec3], closed: bool) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::with_capacity(path.len());
    for p in path {
        if out
            .last()
            .is_none_or(|q| vec2(q.x - p.x, q.z - p.z).length() > 1e-6)
        {
            out.push(*p);
        }
    }
    if closed && out.len() > 1 {
        let (f, l) = (out[0], out[out.len() - 1]);
        if vec2(f.x - l.x, f.z - l.z).length() <= 1e-6 {
            out.pop();
        }
    }
    out
}

/// Samples either side of a vertex whose bend also limits how far the inside of the strip may reach there:
/// without it a straight sample between two tight ones, or the open end of a strip, would still reach out
/// past the centre of curvature next door.
const CLAMP_WINDOW: usize = 2;

fn offset_xz(p: &[Vec2], closed: bool, lateral: f32, o: &StripOpts) -> Vec<Vec2> {
    let n = p.len();
    let closed = closed && n >= 3;
    let limit = o.miter_limit.max(1.);
    let neighbours = |i: usize| {
        let prev = if i > 0 {
            Some(i - 1)
        } else if closed {
            Some(n - 1)
        } else {
            None
        };
        let next = if i + 1 < n {
            Some(i + 1)
        } else if closed {
            Some(0)
        } else {
            None
        };
        (prev, next)
    };
    // Pass 1: how far the inside may reach at each corner. The inner offset point slides back along both
    // segments by `lateral * tan(turn / 2)`; letting each corner use at most `fraction` of half of the
    // shorter segment keeps every offset segment pointing the way the original does.
    let inner_limit: Vec<f32> = (0..n)
        .map(|i| {
            let (prev, next) = neighbours(i);
            let (Some(prev), Some(next)) = (prev, next) else {
                return f32::MAX;
            };
            if o.clamp_to_curvature <= 0. {
                return f32::MAX;
            }
            let (si, so) = (p[i] - p[prev], p[next] - p[i]);
            let (ti, to) = (si.normalize_or_zero(), so.normalize_or_zero());
            let cos_turn = ti.dot(to).clamp(-1., 1.);
            let turn = ti.perp_dot(to); // > 0 turns right: the right side is the inside
            if cos_turn >= 1. - 1e-6 {
                f32::MAX
            } else if turn.abs() < 1e-6 && cos_turn < 0. {
                0. // a hairpin pinches rather than spikes
            } else if lateral * turn > 0. {
                let tan_half = ((1. - cos_turn) / (1. + cos_turn).max(1e-6)).sqrt();
                o.clamp_to_curvature * si.length().min(so.length()) / (2. * tan_half)
            } else {
                f32::MAX
            }
        })
        .collect();
    (0..n)
        .map(|i| {
            let (prev, next) = neighbours(i);
            let seg_in = prev.map(|j| p[i] - p[j]);
            let seg_out = next.map(|j| p[j] - p[i]);
            // Pass 2: the tightest inside limit within the window, for every vertex that is not itself on
            // the outside of its bend.
            let outside = match (seg_in, seg_out) {
                (Some(si), Some(so)) => {
                    lateral * si.normalize_or_zero().perp_dot(so.normalize_or_zero()) < -1e-6
                }
                _ => false,
            };
            let reach = if outside {
                f32::MAX
            } else {
                (0..=2 * CLAMP_WINDOW)
                    .filter_map(|k| {
                        let j = i as isize + k as isize - CLAMP_WINDOW as isize;
                        if closed {
                            Some(inner_limit[j.rem_euclid(n as isize) as usize])
                        } else {
                            inner_limit.get(usize::try_from(j).ok()?).copied()
                        }
                    })
                    .fold(f32::MAX, f32::min)
            };
            let lat = lateral.signum() * lateral.abs().min(reach);
            let off = match (seg_in, seg_out) {
                (None, None) => Vec2::ZERO,
                (Some(s), None) | (None, Some(s)) => right_of(s.normalize_or_zero()) * lat,
                (Some(si), Some(so)) => {
                    let (ni, no) = (
                        right_of(si.normalize_or_zero()),
                        right_of(so.normalize_or_zero()),
                    );
                    let sum = ni + no;
                    let (m, stretch) = if sum.length_squared() < 1e-8 {
                        (no, limit) // a U-turn has no mitre
                    } else {
                        let m = sum.normalize();
                        (m, (1. / m.dot(no).max(1e-6)).min(limit))
                    };
                    m * lat * stretch
                }
            };
            p[i] + off
        })
        .collect()
}

/// The polyline `path` (x, z of each point) moved `lateral` to the right of its direction of travel
/// (negative = left), with mitred corners that cannot fold over themselves.
///
/// * A plain per-segment normal offset leaves gaps on the outside of a bend and crosses over itself on the
///   inside. Here every corner gets the mitre direction (the two segment normals averaged), stretched so
///   the offset keeps its distance from both segments, capped by `miter_limit`.
/// * On the inside of a bend a corner's offset point slides back along both segments by
///   `lateral * tan(turn / 2)`. That is kept under `clamp_to_curvature` times half of the shorter adjacent
///   segment, so each offset segment keeps pointing the way the original does. For a smoothly sampled
///   curve this is a fraction of the local radius of curvature (`R ~ segment length / turn angle`), which
///   is why a border that cannot stay `lateral` away at a tight bend gets closer instead of folding. The
///   clamp is local: a path that crosses *itself* or runs near another part of itself can still overlap;
///   `lint` finds that.
/// * Consecutive points that coincide are merged, so the result can be shorter than `path`. A closed path
///   (`closed`) may repeat its first point at the end. Fewer than three points is never closed.
pub fn offset_path(path: &[Vec2], closed: bool, lateral: f32, opts: &StripOpts) -> Vec<Vec2> {
    let pts: Vec<Vec3> = path.iter().map(|p| vec3(p.x, 0., p.y)).collect();
    let xz: Vec<Vec2> = dedupe(&pts, closed)
        .iter()
        .map(|p| vec2(p.x, p.z))
        .collect();
    offset_xz(&xz, closed, lateral, opts)
}

/// The quads of a ribbon along `path` between `from` and `to` metres to the right of it (negative = left;
/// a centred road of half width `w` is `-w` to `w`). One quad per segment (plus the closing one when
/// `closed`), `[a_i, b_i, b_j, a_j]`, each y taken from the path point plus `opts.lift`. Corners are
/// handled as in [`offset_path`], so the strip cannot fold over itself; hand the result to
/// `lint::strip_folds` to check any strip, this one or your own.
pub fn strip_quads(path: &[Vec3], closed: bool, from: f32, to: f32, opts: &StripOpts) -> Vec<Quad> {
    let pts = dedupe(path, closed);
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let closed = closed && n >= 3;
    let xz: Vec<Vec2> = pts.iter().map(|p| vec2(p.x, p.z)).collect();
    let (a, b) = (
        offset_xz(&xz, closed, from, opts),
        offset_xz(&xz, closed, to, opts),
    );
    let lift = |q: Vec2, i: usize| vec3(q.x, pts[i].y + opts.lift, q.y);
    let segments = if closed { n } else { n - 1 };
    (0..segments)
        .map(|i| {
            let j = (i + 1) % n;
            [lift(a[i], i), lift(b[i], i), lift(b[j], j), lift(a[j], j)]
        })
        .collect()
}

impl Template {
    /// A flat ribbon along a polyline, `half_width` either side of it: roads, kerbs, paths, tracks of
    /// light. `path` is x, z and a y per point (use `opts.lift` to raise it above the ground it lies on).
    /// Triangles face up (+Y) whichever way the path runs; corners are mitred and the inside of a bend is
    /// clamped so the ribbon never folds over itself (see [`offset_path`]), at the price of being narrower
    /// than `half_width` on the inside of a bend tighter than that. Normals are +Y.
    pub fn offset_strip(
        &mut self,
        path: &[Vec3],
        closed: bool,
        half_width: f32,
        opts: &StripOpts,
        c: Rgb,
        e: f32,
    ) {
        self.offset_band(path, closed, -half_width, half_width, opts, c, e);
    }

    /// Like [`Template::offset_strip`] for the band between two lateral offsets from the path (`from` and
    /// `to`, to the right positive): a kerb stripe at `[w, w + 0.4]`, a verge, a lane line. Quads that
    /// collapse to nothing at a clamped bend are skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn offset_band(
        &mut self,
        path: &[Vec3],
        closed: bool,
        from: f32,
        to: f32,
        opts: &StripOpts,
        c: Rgb,
        e: f32,
    ) {
        for q in strip_quads(path, closed, from, to, opts) {
            let area = (q[2] - q[0]).cross(q[3] - q[1]).length() * 0.5;
            if area > 1e-10 {
                // Split along the diagonal that stays inside the quad: a strip bent hard enough leaves a
                // concave quad, and the wrong diagonal would flip one of its two triangles.
                let n1 = (q[1] - q[0]).cross(q[2] - q[0]);
                let n2 = (q[2] - q[0]).cross(q[3] - q[0]);
                let q = if n1.dot(n2) >= 0. {
                    q
                } else {
                    [q[1], q[2], q[3], q[0]]
                };
                self.quad_facing(q, Vec3::Y, c, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volume enclosed by the triangles: positive when they wind outwards, if the mesh is closed.
    fn signed_volume(t: &Template) -> f32 {
        t.idx
            .chunks_exact(3)
            .map(|tri| {
                let p = |i: u16| t.verts[i as usize].p;
                p(tri[0]).dot(p(tri[1]).cross(p(tri[2]))) / 6.
            })
            .sum()
    }

    fn bounds(t: &Template) -> (Vec3, Vec3) {
        t.verts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
            (lo.min(v.p), hi.max(v.p))
        })
    }

    fn assert_wound_by_normals(t: &Template, what: &str) {
        for (i, tri) in t.idx.chunks_exact(3).enumerate() {
            let v = |k: usize| t.verts[tri[k] as usize];
            let g = (v(1).p - v(0).p).cross(v(2).p - v(0).p);
            assert!(
                g.dot(v(0).n + v(1).n + v(2).n) >= 0.,
                "{what}: triangle {i} faces against its normals"
            );
        }
        for v in &t.verts {
            assert!(
                (v.n.length() - 1.).abs() < 1e-3,
                "{what}: non-unit normal {}",
                v.n
            );
        }
    }

    const C: Rgb = [0.5; 3];

    #[test]
    fn a_lofted_cylinder_is_a_closed_outward_solid_of_the_right_volume() {
        let mut t = Template::new();
        let secs: Vec<Section> = [0., 1., 2.]
            .iter()
            .map(|z| Section::across_z(*z, 0., 0., 1., 1., 0.))
            .collect();
        t.loft(&secs, Ring::Ellipse(32), true, C, 0.);
        let area = 0.5 * 32. * (TAU / 32.).sin();
        assert!(
            (signed_volume(&t) - area * 2.).abs() < 0.01,
            "volume {} vs {}",
            signed_volume(&t),
            area * 2.
        );
        assert_wound_by_normals(&t, "loft");
        // Side normals are radial, caps are axial.
        let (lo, hi) = bounds(&t);
        assert!((hi.z - lo.z - 2.).abs() < 1e-5);
    }

    #[test]
    fn loft_tapers_to_a_point_without_degenerate_triangles() {
        // A bullet: a point at the tip. Nothing zero-area may be emitted.
        let mut t = Template::new();
        let secs = [
            Section::across_z(0., 0., 0., 0.5, 0.5, 0.),
            Section::across_z(1., 0., 0., 0.3, 0.3, 0.),
            Section::across_z(1.5, 0., 0., 0., 0., 0.),
        ];
        t.loft(&secs, Ring::Ellipse(12), true, C, 0.);
        assert!(signed_volume(&t) > 0.);
        for tri in t.idx.chunks_exact(3) {
            let p = |k: usize| t.verts[tri[k] as usize].p;
            assert!((p(1) - p(0)).cross(p(2) - p(0)).length() > 1e-6);
        }
    }

    #[test]
    fn loft_with_fewer_than_two_sections_adds_nothing() {
        let mut t = Template::new();
        t.loft(&[], Ring::Ellipse(8), true, C, 0.);
        t.loft(
            &[Section::across_z(0., 0., 0., 1., 1., 0.)],
            Ring::Ellipse(8),
            true,
            C,
            0.,
        );
        assert!(t.is_empty());
    }

    #[test]
    fn rounded_box_keeps_its_outer_size_and_loses_only_the_corners() {
        let mut t = Template::new();
        let (centre, half) = (vec3(1., 2., 3.), vec3(0.5, 1., 1.5));
        t.rounded_box(centre, half, 0.2, C, 0.);
        let (lo, hi) = bounds(&t);
        assert!((lo - (centre - half)).abs().max_element() < 1e-4, "{lo}");
        assert!((hi - (centre + half)).abs().max_element() < 1e-4, "{hi}");
        let full = 8. * half.x * half.y * half.z;
        let v = signed_volume(&t);
        assert!(v > 0.9 * full && v < full, "volume {v} of {full}");
        assert_wound_by_normals(&t, "rounded_box");
    }

    #[test]
    fn a_rounded_box_with_an_oversized_radius_becomes_a_pill_not_garbage() {
        let mut t = Template::new();
        t.rounded_box(Vec3::ZERO, vec3(0.2, 0.5, 0.2), 5., C, 0.);
        let (lo, hi) = bounds(&t);
        assert!((hi - lo - vec3(0.4, 1., 0.4)).abs().max_element() < 1e-3);
        assert!(t.verts.iter().all(|v| v.p.is_finite() && v.n.is_finite()));
    }

    #[test]
    fn capsule_has_the_volume_and_extent_of_a_capsule_and_faces_outwards() {
        let mut t = Template::new();
        let (a, b, r) = (vec3(0., 0., 0.), vec3(0., 2., 0.), 0.5);
        t.capsule(a, b, r, C, 0., 24);
        let (lo, hi) = bounds(&t);
        assert!(
            (lo.y + r).abs() < 1e-4 && (hi.y - 2. - r).abs() < 1e-4,
            "{lo} {hi}"
        );
        let exact = PI * r * r * 2. + 4. / 3. * PI * r * r * r;
        let v = signed_volume(&t);
        assert!(v > 0.93 * exact && v < exact, "volume {v} vs {exact}");
        assert_wound_by_normals(&t, "capsule");
        // Any axis direction, including exactly along Y (the basis helper's special case) and a point.
        for end in [vec3(1., 1., 1.), vec3(0., -3., 0.), a] {
            let mut t = Template::new();
            t.capsule(a, end, 0.2, C, 0., 12);
            assert!(!t.is_empty());
            assert!(t.verts.iter().all(|v| v.p.is_finite() && v.n.is_finite()));
            assert!(signed_volume(&t) > 0.);
        }
    }

    #[test]
    fn rod_runs_between_two_points() {
        let mut t = Template::new();
        t.rod(vec3(0., 0., 0.), vec3(0., 0., -2.), 0.1, 0.05, C, 0., 8);
        let (lo, hi) = bounds(&t);
        assert!((lo.z + 2.).abs() < 1e-5 && hi.z.abs() < 1e-5);
        assert!(hi.x <= 0.1 + 1e-5);
        let mut empty = Template::new();
        empty.rod(Vec3::ONE, Vec3::ONE, 0.1, 0.1, C, 0., 8);
        assert!(empty.is_empty());
    }

    #[test]
    fn sweep_follows_a_curved_path_and_closes_a_loop() {
        // A wire along a quarter circle, then a full ring.
        let path = arc(Vec3::ZERO, Vec3::X, Vec3::Y, [1., 1.], 0., FRAC_PI_2, 8);
        let mut t = Template::new();
        t.sweep(&path, &Sweep::round(Vec3::Z, 0.05), C, 0.);
        let expected = PI * 0.05 * 0.05 * (PI / 2.); // pi r^2 * length
        let v = signed_volume(&t);
        assert!(
            v > 0.85 * expected && v < 1.1 * expected,
            "volume {v} vs {expected}"
        );
        assert_wound_by_normals(&t, "sweep");

        let ring = arc(Vec3::ZERO, Vec3::X, Vec3::Y, [1., 1.], 0., TAU, 24);
        let mut loopt = Template::new();
        loopt.sweep(&ring, &Sweep::round(Vec3::Z, 0.05), C, 0.);
        assert!(signed_volume(&loopt) > 0.85 * PI * 0.05 * 0.05 * TAU);
        // A closed loop has no end caps: 24 rings of 8 points and nothing else.
        assert_eq!(loopt.verts.len(), 24 * 8);
        assert_wound_by_normals(&loopt, "closed sweep");
    }

    #[test]
    fn sweep_tapers_and_survives_duplicate_points() {
        let path = [
            Vec3::ZERO,
            Vec3::ZERO,
            vec3(0., 1., 0.),
            vec3(0., 2., 0.),
            vec3(0.5, 3., 0.),
        ];
        let profile = Sweep {
            side: Vec3::Z,
            half_width: [0.2, 0.05],
            half_height: [0.2, 0.05],
            ring: Ring::Rounded(1),
        };
        let mut t = Template::new();
        t.sweep(&path, &profile, C, 0.);
        assert!(!t.is_empty());
        assert!(t.verts.iter().all(|v| v.p.is_finite() && v.n.is_finite()));
        assert!(signed_volume(&t) > 0.);
        let mut none = Template::new();
        none.sweep(&[Vec3::ONE], &profile, C, 0.);
        none.sweep(&[Vec3::ONE, Vec3::ONE], &profile, C, 0.);
        assert!(none.is_empty());
    }

    #[test]
    fn quad_bezier_and_arc_hit_their_end_points() {
        let b = quad_bezier(Vec3::ZERO, vec3(1., 0., 0.), vec3(1., 1., 0.), 6);
        assert_eq!(b.len(), 7);
        assert!((b[0]).length() < 1e-6 && (b[6] - vec3(1., 1., 0.)).length() < 1e-6);
        let a = arc(vec3(1., 0., 0.), Vec3::X, Vec3::Z, [2., 2.], 0., PI, 4);
        assert!(
            (a[0] - vec3(3., 0., 0.)).length() < 1e-5 && (a[4] - vec3(-1., 0., 0.)).length() < 1e-5
        );
    }

    #[test]
    fn mirroring_a_half_doubles_it_and_keeps_every_triangle_facing_outwards() {
        let mut half = Template::new();
        half.rounded_box(vec3(1., 0., 0.), vec3(0.5, 0.5, 0.5), 0.1, C, 0.);
        let single = signed_volume(&half);
        let mirrored = half.mirrored_x();
        assert!(
            (signed_volume(&mirrored) - single).abs() < 1e-4,
            "the mirror image has the same positive volume, so the winding was reversed too"
        );
        assert_wound_by_normals(&mirrored, "mirrored");
        half.mirror_x();
        assert!((signed_volume(&half) - 2. * single).abs() < 1e-4);
        let (lo, hi) = bounds(&half);
        assert!((lo.x + hi.x).abs() < 1e-5, "symmetric about x = 0");
        // The reflected half's normal x components are the negation of the original's.
        let n = half.verts.len() / 2;
        for i in 0..n {
            assert!((half.verts[i].n.x + half.verts[n + i].n.x).abs() < 1e-6);
        }
    }

    #[test]
    fn smooth_normals_welds_a_box_into_eight_corners_when_the_angle_allows() {
        let mut hard = Template::new();
        hard.box_(Vec3::ZERO, Vec3::ONE, C, 0.);
        let faces = hard.clone();
        // 45 degrees keeps the 90 degree edges hard: nothing changes but the count stays 24.
        let mut kept = faces.clone();
        kept.smooth_normals(45.);
        assert_eq!(kept.verts.len(), 24);
        for v in &kept.verts {
            assert!(v.n.abs().max_element() > 0.999, "{}", v.n);
        }
        // 100 degrees blends them: 8 welded corners with diagonal normals.
        let mut soft = faces.clone();
        soft.smooth_normals(100.);
        assert_eq!(soft.verts.len(), 8);
        assert_eq!(soft.idx.len(), 36);
        for v in &soft.verts {
            assert!(
                (v.n.abs() - Vec3::splat(0.57735)).abs().max_element() < 1e-3,
                "{}",
                v.n
            );
        }
        assert_wound_by_normals(&soft, "smoothed box");
    }

    #[test]
    fn smooth_normals_rebuilds_radial_normals_from_the_winding_alone() {
        // Wreck the stored normals of a capsule: the winding is all smooth_normals needs. A side vertex
        // at azimuth 0 ends up with a normal pointing along +X, tilted only by the hemisphere's facets.
        let mut t = Template::new();
        t.capsule(Vec3::ZERO, vec3(0., 1., 0.), 0.5, C, 0., 12);
        for v in &mut t.verts {
            v.n = Vec3::Y;
        }
        t.smooth_normals(60.);
        let side = t
            .verts
            .iter()
            .find(|v| (v.p.y - 1.).abs() < 1e-3 && v.p.z.abs() < 1e-3 && v.p.x > 0.4)
            .unwrap();
        assert!(side.n.x > 0.9 && side.n.z.abs() < 1e-2, "{}", side.n);
        // The seam vertices (azimuth 0 and 360) were welded, so no vertex is duplicated by position+normal.
        let mut keys: Vec<_> = t
            .verts
            .iter()
            .map(|v| {
                (
                    (v.p * 1e3).round().to_array().map(|f| f as i32),
                    (v.n * 100.).round().to_array().map(|f| f as i32),
                )
            })
            .collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(before, keys.len());
        assert_wound_by_normals(&t, "smoothed capsule");
    }

    #[test]
    fn a_strip_along_a_straight_path_is_exactly_as_wide_as_asked_and_faces_up_both_ways() {
        for path in [
            vec![vec3(0., 1., 0.), vec3(0., 1., -5.), vec3(0., 1., -10.)],
            vec![vec3(0., 1., -10.), vec3(0., 1., -5.), vec3(0., 1., 0.)],
        ] {
            let mut t = Template::new();
            t.offset_strip(&path, false, 2., &StripOpts::default(), C, 0.);
            assert_eq!(t.idx.len(), 12);
            let (lo, hi) = bounds(&t);
            assert!((hi.x - lo.x - 4.).abs() < 1e-5);
            assert!(t.verts.iter().all(|v| v.p.y == 1. && v.n == Vec3::Y));
            for tri in t.idx.chunks_exact(3) {
                let p = |k: usize| t.verts[tri[k] as usize].p;
                assert!((p(1) - p(0)).cross(p(2) - p(0)).y > 0., "upward winding");
            }
        }
    }

    #[test]
    fn positive_lateral_is_to_the_right_of_travel() {
        // Travelling towards -Z, seen from above, +X is on the right.
        let pts = [vec2(0., 0.), vec2(0., -1.)];
        let o = offset_path(&pts, false, 0.5, &StripOpts::default());
        assert!((o[0] - vec2(0.5, 0.)).length() < 1e-6 && (o[1] - vec2(0.5, -1.)).length() < 1e-6);
        let o = offset_path(&pts, false, -0.5, &StripOpts::default());
        assert!((o[0] - vec2(-0.5, 0.)).length() < 1e-6);
    }

    #[test]
    fn a_square_corner_is_mitred_so_both_edges_keep_their_distance() {
        // North, then east: a right turn. The outer (left) offset needs the mitre.
        let pts = [vec2(0., 0.), vec2(0., -4.), vec2(4., -4.)];
        let o = offset_path(&pts, false, -1., &StripOpts::default());
        assert!(
            (o[1] - vec2(-1., -5.)).length() < 1e-5,
            "outer corner {}",
            o[1]
        );
        let i = offset_path(&pts, false, 1., &StripOpts::default());
        assert!(
            (i[1] - vec2(1., -3.)).length() < 1e-5,
            "inner corner {}",
            i[1]
        );
    }

    #[test]
    fn the_miter_limit_stops_a_sharp_corner_from_spiking() {
        // A 150 degree turn: an unlimited mitre is 1 / cos(75 deg) = 3.9 times the offset.
        let a = 150f32.to_radians();
        let pts = [
            vec2(0., 0.),
            vec2(0., -4.),
            vec2(4. * a.sin(), -4. - 4. * a.cos()),
        ];
        let limited = offset_path(&pts, false, -1., &StripOpts::default());
        assert!(
            (limited[1] - pts[1]).length() <= 2.0001,
            "{}",
            (limited[1] - pts[1]).length()
        );
        let loose = offset_path(
            &pts,
            false,
            -1.,
            &StripOpts {
                miter_limit: 10.,
                ..Default::default()
            },
        );
        assert!((loose[1] - pts[1]).length() > 3.5);
    }

    fn circle(r: f32, n: usize) -> Vec<Vec3> {
        (0..n)
            .map(|i| {
                let a = TAU * i as f32 / n as f32;
                vec3(r * a.cos(), 0., r * a.sin())
            })
            .collect()
    }

    /// Per-sample offsets along the averaged normal with no stretch and no clamp: the bug being fixed.
    fn naive_quads(path: &[Vec3], closed: bool, from: f32, to: f32) -> Vec<Quad> {
        let n = path.len();
        let normal = |i: usize| {
            let (p, q) = (path[(i + n - 1) % n], path[(i + 1) % n]);
            let t = vec2(q.x - p.x, q.z - p.z).normalize_or_zero();
            right_of(t)
        };
        let at = |i: usize, l: f32| {
            let o = normal(i) * l;
            vec3(path[i].x + o.x, path[i].y, path[i].z + o.y)
        };
        let segs = if closed { n } else { n - 1 };
        (0..segs)
            .map(|i| {
                let j = (i + 1) % n;
                [at(i, from), at(i, to), at(j, to), at(j, from)]
            })
            .collect()
    }

    #[test]
    fn a_wide_strip_round_a_tight_circle_never_folds_when_clamped_and_does_without() {
        use crate::viewer::kit::lint::strip_folds;
        // Radius 3, half width 8: the inner edge would pass the centre of the circle and turn inside out.
        let path = circle(3., 24);
        let naive = naive_quads(&path, true, -8., 8.);
        assert!(
            !strip_folds(&naive).is_empty(),
            "the naive offset must fold here"
        );
        let quads = strip_quads(&path, true, -8., 8., &StripOpts::default());
        assert_eq!(quads.len(), 24);
        assert!(strip_folds(&quads).is_empty(), "{:?}", strip_folds(&quads));
        quads.iter().for_each(assert_flat_up_or_zero);
        // Both sides, and the same with the clamp off folding again (so the clamp is what does it).
        let off = StripOpts {
            clamp_to_curvature: 0.,
            ..Default::default()
        };
        assert!(!strip_folds(&strip_quads(&path, true, -8., 8., &off)).is_empty());
    }

    fn assert_flat_up_or_zero(q: &Quad) {
        let g = (q[1] - q[0]).cross(q[2] - q[0]) + (q[2] - q[0]).cross(q[3] - q[0]);
        assert!(g.y >= -1e-4, "{q:?}");
    }

    #[test]
    fn the_inner_edge_stays_near_the_requested_distance_where_the_bend_allows_it() {
        // A gentle circle (radius 50): the clamp must not bite at a 2 m half width.
        let path = circle(50., 64);
        let o = offset_path(
            &path.iter().map(|p| vec2(p.x, p.z)).collect::<Vec<_>>(),
            true,
            2.,
            &StripOpts::default(),
        );
        for (p, q) in path.iter().zip(&o) {
            let d = vec2(p.x, p.z).length() - q.length();
            assert!((d - 2.).abs() < 0.01, "inner offset {d}");
        }
        // The right of a counter-clockwise (seen from +Y with +X right, +Z down) circle: with this
        // sample order the loop turns right, so the right side is the inside, as asserted above.
    }

    #[test]
    fn random_wiggly_paths_never_fold_open_or_closed_and_their_templates_are_lint_clean() {
        use crate::viewer::kit::lint::{lint, strip_folds, Defect};
        let mut seed = 0x2545f4914f6cdd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        for case in 0..60 {
            // A walk with random turns of up to 140 degrees and varied segment lengths.
            let mut p = vec3(0., 0.2, 0.);
            let mut heading = 0f32;
            let mut path = vec![p];
            for _ in 0..(6 + case % 20) {
                heading += (rnd() - 0.5) * 2.4;
                p += vec3(heading.cos(), 0., heading.sin()) * (0.5 + rnd() * 4.);
                path.push(p);
            }
            let width = 0.2 + rnd() * 6.;
            for closed in [false, true] {
                let quads = strip_quads(&path, closed, -width, width, &StripOpts::default());
                assert!(
                    strip_folds(&quads).is_empty(),
                    "case {case} closed {closed}: {:?}",
                    strip_folds(&quads)
                );
            }
            // Open strips of a non-self-crossing path are also free of overlaps; check the simplest:
            // one-sided bands (a kerb on the outside edge) of the open path.
            let mut t = Template::new();
            t.offset_band(&path, false, 0., width, &StripOpts::default(), C, 0.);
            let folds = lint(&t)
                .into_iter()
                .filter(|d| matches!(d, Defect::WindingMismatch { .. } | Defect::ZeroArea { .. }))
                .count();
            assert_eq!(folds, 0, "case {case}");
        }
    }

    #[test]
    fn a_strip_with_coincident_points_or_too_few_points_is_harmless() {
        let mut t = Template::new();
        t.offset_strip(&[], false, 1., &StripOpts::default(), C, 0.);
        t.offset_strip(&[Vec3::ZERO], false, 1., &StripOpts::default(), C, 0.);
        t.offset_strip(
            &[Vec3::ZERO, Vec3::ZERO],
            true,
            1.,
            &StripOpts::default(),
            C,
            0.,
        );
        assert!(t.is_empty());
        let path = [
            vec3(0., 0., 0.),
            vec3(0., 0., 0.),
            vec3(0., 0., -3.),
            vec3(0., 0., -3.),
        ];
        t.offset_strip(&path, false, 1., &StripOpts::default(), C, 0.);
        assert_eq!(t.idx.len(), 6, "duplicates merged: one segment, one quad");
        // A closed path that repeats its first point is the same as one that does not.
        let a = strip_quads(&circle(5., 12), true, -1., 1., &StripOpts::default());
        let mut again = circle(5., 12);
        again.push(again[0]);
        let b = strip_quads(&again, true, -1., 1., &StripOpts::default());
        assert_eq!(a, b);
    }

    #[test]
    fn lift_raises_the_strip_and_a_hairpin_pinches_instead_of_spiking() {
        let o = StripOpts {
            lift: 0.05,
            ..Default::default()
        };
        let q = strip_quads(&[vec3(0., 1., 0.), vec3(0., 1., -1.)], false, -1., 1., &o);
        assert!(q[0].iter().all(|p| (p.y - 1.05).abs() < 1e-6));
        // Out and straight back: the outer offset may not shoot off to infinity.
        let hairpin = [vec2(0., 0.), vec2(0., -3.), vec2(0., 0.)];
        for lateral in [-1., 1.] {
            let r = offset_path(&hairpin, false, lateral, &StripOpts::default());
            assert!(r.iter().all(|p| p.is_finite() && p.length() < 10.), "{r:?}");
        }
    }
}
