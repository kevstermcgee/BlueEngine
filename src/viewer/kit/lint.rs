//! A geometry-defect lint for [`Template`]s: the mesh bugs a headless test can catch before anyone has to
//! look at a screenshot.
//!
//! Three shipped bugs motivated it, none of which any other test could see:
//!
//! * **Coplanar surfaces z-fight.** Spooky Kart drew its kerb stripes in the same plane (y = 0) as the road
//!   quad underneath them. Which of the two wins each pixel flips with the camera, so the kerbs flickered
//!   and crawled. [`Defect::CoplanarOverlap`] finds two front-facing triangles of *different parts* that
//!   lie in one plane (within [`LintConfig::plane_epsilon`]) and cover common area.
//! * **Folded strips.** The road border offset every sample along its own segment normal with no corner
//!   join, so at a tight bend the inner edge crossed itself and the strip turned inside out.
//!   [`strip_folds`] checks a strip of [`Quad`]s for that; [`Template::offset_strip`] builds strips that
//!   pass it.
//! * **Wrong winding.** A triangle wound against its own vertex normals is culled, drawn dark, or lit from
//!   behind. [`Defect::WindingMismatch`].
//!
//! Plus zero-area triangles. Run it in a unit test over the templates a game builds:
//!
//! ```ignore
//! kit::lint::assert_clean(&road_template, "road");     // panics with a readable list
//! let defects = kit::lint::lint(&template);            // or inspect them
//! ```
//!
//! It is geometry only and says nothing about depth precision of the *camera*: [`depth_resolution`] and
//! [`View::camera_checked`](super::View::camera_checked) cover that half. Two coplanar surfaces a millimetre
//! apart still fight at 100 m with a typical near plane, so raise [`LintConfig::plane_epsilon`] to the depth
//! resolution at your farthest viewing distance for a strict check.
use super::batch::Template;
use super::shape::Quad;
use macroquad::prelude::*;
use std::collections::HashMap;
use std::fmt;

/// One finding. Triangle numbers index `Template::idx` in threes (triangle `t` is `idx[3t..3t + 3]`);
/// quad numbers index the slice given to [`strip_folds`].
#[derive(Clone, Debug, PartialEq)]
pub enum Defect {
    /// The triangle names a vertex that does not exist (or the index list ends mid-triangle).
    BadIndex {
        /// Triangle number.
        tri: usize,
    },
    /// The triangle covers no area (collinear or coincident points). Harmless to draw, but it usually means
    /// a loft or strip collapsed where it should not have.
    ZeroArea {
        /// Triangle number.
        tri: usize,
    },
    /// The triangle's winding disagrees with its vertex normals: it faces away from where it claims to.
    WindingMismatch {
        /// Triangle number.
        tri: usize,
    },
    /// Two triangles of different parts face the same way, lie in one plane and overlap: z-fighting.
    /// Reported once per pair of parts (a *part* is a set of triangles connected through shared
    /// vertices; separate quads are separate parts), with the first offending triangle pair and the
    /// total overlapping area of the two parts.
    CoplanarOverlap {
        /// A triangle of the first part.
        tri_a: usize,
        /// A triangle of the second part.
        tri_b: usize,
        /// Overlapping area summed over the two parts, in squared template units.
        area: f32,
    },
    /// A quad of a strip is folded: twisted into a bow tie, running backwards along the strip, or facing
    /// the opposite way from its neighbour. See [`strip_folds`].
    StripFold {
        /// Quad number within the strip.
        quad: usize,
    },
    /// The list stopped here: `shown` findings are listed, more exist (or a work cap was reached).
    Truncated {
        /// How many findings precede this marker.
        shown: usize,
    },
}

impl fmt::Display for Defect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Defect::BadIndex { tri } => write!(f, "triangle {tri}: index out of range"),
            Defect::ZeroArea { tri } => write!(f, "triangle {tri}: zero area"),
            Defect::WindingMismatch { tri } => {
                write!(f, "triangle {tri}: wound against its vertex normals")
            }
            Defect::CoplanarOverlap { tri_a, tri_b, area } => write!(
                f,
                "triangles {tri_a} and {tri_b}: coplanar and overlapping ({area:.4} units^2): z-fighting"
            ),
            Defect::StripFold { quad } => write!(f, "strip quad {quad}: folded over itself"),
            Defect::Truncated { shown } => {
                write!(f, "... more findings after the first {shown}")
            }
        }
    }
}

/// Tolerances and caps for [`lint_with`]. Units are the template's (metres in the examples).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LintConfig {
    /// Triangles smaller than this area count as zero-area. Default 1e-10.
    pub area_epsilon: f32,
    /// Do not report a zero-area triangle that has two coincident corners: the pole of a sphere, the tip of
    /// a cone, a loft section of size zero. They draw nothing and are expected from `Template::ball` and
    /// `Template::cone`. A triangle whose three distinct corners are collinear is always reported.
    /// Default true.
    pub allow_collapsed_poles: bool,
    /// Two triangles are coplanar when every corner of each is within this distance of the other's
    /// plane. Default 0.001, one millimetre in metre units. At distance a depth buffer cannot tell planes
    /// further apart than [`depth_resolution`] says: raise this for a strict check.
    pub plane_epsilon: f32,
    /// Triangles facing within this many degrees of each other count as parallel. Default 1.
    pub max_angle_deg: f32,
    /// Overlaps smaller than this area are ignored (shared edges, rounding noise). Default 1e-6, a square
    /// millimetre in metre units: lower it for tiny models.
    pub min_overlap_area: f32,
    /// Stop collecting after this many findings and end the list with [`Defect::Truncated`]. Default 100.
    pub max_defects: usize,
    /// Work cap on candidate triangle pairs examined for overlap; reaching it ends the list with
    /// [`Defect::Truncated`]. Default 20 million (a second or two at worst).
    pub max_pair_tests: usize,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            area_epsilon: 1e-10,
            allow_collapsed_poles: true,
            plane_epsilon: 1e-3,
            max_angle_deg: 1.,
            min_overlap_area: 1e-6,
            max_defects: 100,
            max_pair_tests: 20_000_000,
        }
    }
}

/// Lint a template with the default tolerances. Empty means nothing was found.
pub fn lint(t: &Template) -> Vec<Defect> {
    lint_with(t, &LintConfig::default())
}

/// Panic with a readable list of findings unless [`lint`] is clean. `what` names the template in the message.
/// For unit tests: `assert_clean(&road, "road")`.
pub fn assert_clean(t: &Template, what: &str) {
    let defects = lint(t);
    assert!(
        defects.is_empty(),
        "{what}: {} geometry defects:\n  {}",
        defects.len(),
        defects
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

struct Tri {
    n: Vec3,
    p: [Vec3; 3],
    lo: Vec3,
    hi: Vec3,
    part: usize,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Lint a template with explicit tolerances.
pub fn lint_with(t: &Template, cfg: &LintConfig) -> Vec<Defect> {
    let mut out: Vec<Defect> = Vec::new();
    let mut truncated = false;
    let n_verts = t.verts.len();
    if !t.idx.len().is_multiple_of(3) {
        out.push(Defect::BadIndex {
            tri: t.idx.len() / 3,
        });
    }
    // Parts: triangles connected through shared vertex indices.
    let mut parent: Vec<usize> = (0..n_verts).collect();
    for tri in t.idx.chunks_exact(3) {
        let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        if ids.iter().all(|&i| i < n_verts) {
            let (a, b, c) = (
                find(&mut parent, ids[0]),
                find(&mut parent, ids[1]),
                find(&mut parent, ids[2]),
            );
            parent[b] = a;
            parent[c] = a;
        }
    }
    let mut tris: Vec<(usize, Tri)> = Vec::new(); // (triangle number, data) for non-degenerate ones
    for (ti, tri) in t.idx.chunks_exact(3).enumerate() {
        let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        if ids.iter().any(|&i| i >= n_verts) {
            out.push(Defect::BadIndex { tri: ti });
            continue;
        }
        let v = [t.verts[ids[0]], t.verts[ids[1]], t.verts[ids[2]]];
        let p = [v[0].p, v[1].p, v[2].p];
        let g = (p[1] - p[0]).cross(p[2] - p[0]);
        // Two coincident corners: a pole or tip. In f32 the "same" pole can differ by 1e-7, which makes a
        // sliver whose winding is noise, so these are set aside before any other check.
        let collapsed = (0..3).any(|k| (p[k] - p[(k + 1) % 3]).length_squared() < 1e-10);
        if collapsed && cfg.allow_collapsed_poles {
            continue;
        }
        if g.length() * 0.5 < cfg.area_epsilon {
            out.push(Defect::ZeroArea { tri: ti });
            continue;
        }
        let n = g.normalize();
        let sum = v[0].n + v[1].n + v[2].n;
        if sum.length_squared() > 1e-12 && n.dot(sum.normalize()) < -1e-3 {
            out.push(Defect::WindingMismatch { tri: ti });
        }
        let eps = Vec3::splat(cfg.plane_epsilon);
        tris.push((
            ti,
            Tri {
                n,
                p,
                lo: p[0].min(p[1]).min(p[2]) - eps,
                hi: p[0].max(p[1]).max(p[2]) + eps,
                part: find(&mut parent, ids[0]),
            },
        ));
    }

    // Coplanar overlaps between parts: sweep over x, test the cheap conditions first.
    let cos_max = cfg.max_angle_deg.to_radians().cos();
    let mut order: Vec<usize> = (0..tris.len()).collect();
    order.sort_by(|&a, &b| tris[a].1.lo.x.total_cmp(&tris[b].1.lo.x));
    let mut active: Vec<usize> = Vec::new();
    let mut pairs: HashMap<(usize, usize), (usize, usize, f32)> = HashMap::new();
    let mut tests = 0usize;
    'sweep: for &i in &order {
        let a = &tris[i].1;
        active.retain(|&j| tris[j].1.hi.x >= a.lo.x);
        for &j in &active {
            let b = &tris[j].1;
            if a.part == b.part
                || a.lo.y > b.hi.y
                || b.lo.y > a.hi.y
                || a.lo.z > b.hi.z
                || b.lo.z > a.hi.z
            {
                continue;
            }
            tests += 1;
            if tests > cfg.max_pair_tests {
                truncated = true;
                break 'sweep;
            }
            if a.n.dot(b.n) < cos_max || !coplanar(a, b, cfg.plane_epsilon) {
                continue;
            }
            let area = overlap_area(a, b);
            if area >= cfg.min_overlap_area {
                let key = (a.part.min(b.part), a.part.max(b.part));
                let (ta, tb) = (tris[i].0.min(tris[j].0), tris[i].0.max(tris[j].0));
                let e = pairs.entry(key).or_insert((ta, tb, 0.));
                e.2 += area;
                if (ta, tb) < (e.0, e.1) {
                    e.0 = ta;
                    e.1 = tb;
                }
            }
        }
        active.push(i);
    }
    let mut overlaps: Vec<(usize, usize, f32)> = pairs.into_values().collect();
    overlaps.sort_by(|a, b| b.2.total_cmp(&a.2).then((a.0, a.1).cmp(&(b.0, b.1))));
    out.extend(
        overlaps
            .into_iter()
            .map(|(tri_a, tri_b, area)| Defect::CoplanarOverlap { tri_a, tri_b, area }),
    );

    if out.len() > cfg.max_defects {
        out.truncate(cfg.max_defects);
        truncated = true;
    }
    if truncated {
        let shown = out.len();
        out.push(Defect::Truncated { shown });
    }
    out
}

fn coplanar(a: &Tri, b: &Tri, eps: f32) -> bool {
    a.p.iter().all(|p| (*p - b.p[0]).dot(b.n).abs() <= eps)
        && b.p.iter().all(|p| (*p - a.p[0]).dot(a.n).abs() <= eps)
}

/// Area where two (nearly) coplanar triangles overlap, by clipping one against the other in the plane's
/// dominant 2D projection. Done in f64 so triangles that merely share an edge (identical corners) clip to
/// exactly nothing instead of an f32 sliver.
fn overlap_area(a: &Tri, b: &Tri) -> f32 {
    type P2 = [f64; 2];
    let n = a.n.abs();
    let drop = if n.x >= n.y && n.x >= n.z {
        0
    } else if n.y >= n.z {
        1
    } else {
        2
    };
    let project = |p: Vec3| -> P2 {
        let (x, y, z) = (p.x as f64, p.y as f64, p.z as f64);
        match drop {
            0 => [y, z],
            1 => [z, x],
            _ => [x, y],
        }
    };
    let cross = |o: P2, p: P2, q: P2| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
    let ccw = |p: [Vec3; 3]| -> Vec<P2> {
        let q = [project(p[0]), project(p[1]), project(p[2])];
        if cross(q[0], q[1], q[2]) < 0. {
            vec![q[0], q[2], q[1]]
        } else {
            q.to_vec()
        }
    };
    let clip = ccw(a.p);
    let mut poly = ccw(b.p);
    for i in 0..3 {
        let (c0, c1) = (clip[i], clip[(i + 1) % 3]);
        let side = |p: P2| cross(c0, c1, p);
        let input = std::mem::take(&mut poly);
        for k in 0..input.len() {
            let (p, q) = (input[k], input[(k + 1) % input.len()]);
            let (dp, dq) = (side(p), side(q));
            if (dp >= 0.) != (dq >= 0.) {
                let t = dp / (dp - dq);
                poly.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
            if dq >= 0. {
                poly.push(q);
            }
        }
        if poly.is_empty() {
            return 0.;
        }
    }
    let mut area = 0.;
    for k in 0..poly.len() {
        let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
        area += p[0] * q[1] - p[1] * q[0];
    }
    (area.abs() * 0.5) as f32
}

/// Fold-over in a strip of quads (`[a_i, b_i, b_j, a_j]` per segment, as [`strip_quads`](super::shape::strip_quads)
/// makes them): the finding the road border had at tight bends. A quad is reported when
///
/// * its opposite edges cross (a bow tie: the corners are twisted),
/// * either long edge (`a_i -> a_j`, `b_i -> b_j`) runs backwards compared with the quad's own direction of
///   travel (the inner side of a bend passed the centre of curvature), or
/// * it faces the opposite way from the previous quad (the strip turned inside out).
///
/// It does not look for a strip crossing a *different* part of itself: [`lint`] finds that as a coplanar
/// overlap once the quads are in a template. Quads of zero area are not folds.
pub fn strip_folds(strip: &[Quad]) -> Vec<Defect> {
    let mut out = Vec::new();
    let mut prev: Option<Vec3> = None;
    for (i, q) in strip.iter().enumerate() {
        let n1 = (q[1] - q[0]).cross(q[2] - q[0]);
        let n2 = (q[2] - q[0]).cross(q[3] - q[0]);
        let travel = (q[2] + q[3]) * 0.5 - (q[0] + q[1]) * 0.5;
        let (ea, eb) = (q[3] - q[0], q[2] - q[1]);
        let reversed = |e: Vec3| e.length() > 1e-6 && e.dot(travel) < 0.;
        let mut fold = reversed(ea)
            || reversed(eb)
            || bow_tie(
                q,
                if n1.length_squared() >= n2.length_squared() {
                    n1
                } else {
                    n2
                },
            );
        // The polygon's own signed area decides which way the quad faces (right even when concave).
        let n = n1 + n2;
        if n.length_squared() > 1e-16 {
            if let Some(p) = prev {
                if p.dot(n) < 0. {
                    fold = true;
                }
            }
            prev = Some(n);
        }
        if fold {
            out.push(Defect::StripFold { quad: i });
        }
    }
    out
}

/// True when opposite edges of the quad cross each other (a twisted, self-intersecting quad), seen along
/// `normal`. A concave but simple quad is not a bow tie.
fn bow_tie(q: &Quad, normal: Vec3) -> bool {
    let n = normal.abs();
    let drop = if n.x >= n.y && n.x >= n.z {
        0
    } else if n.y >= n.z {
        1
    } else {
        2
    };
    let p = |v: Vec3| match drop {
        0 => vec2(v.y, v.z),
        1 => vec2(v.z, v.x),
        _ => vec2(v.x, v.y),
    };
    let crosses = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        let side = |s: Vec2, t: Vec2, u: Vec2| (t - s).perp_dot(u - s);
        side(c, d, a) * side(c, d, b) < 0. && side(a, b, c) * side(a, b, d) < 0.
    };
    let [a, b, c, d] = [p(q[0]), p(q[1]), p(q[2]), p(q[3])];
    crosses(a, b, c, d) || crosses(b, c, d, a)
}

/// How far apart in depth two surfaces must be for a 24-bit depth buffer to tell them apart, `distance`
/// from the camera, with the near plane at `near` (perspective depth: `distance^2 / (near * 2^24)`).
/// A kerb lifted 2 mm off a road is resolved at 5 m with `near = 0.1` (0.015 mm) but not at 100 m (6 mm),
/// and a `near` of 0.05 doubles the problem. Put the near plane as far out as the game allows.
pub fn depth_resolution(near: f32, distance: f32) -> f32 {
    distance * distance / (near.max(1e-6) * 16_777_216.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::kit::shape::{strip_quads, StripOpts};

    const C: [f32; 3] = [0.5; 3];

    fn road_with_kerb(kerb_y: f32) -> Template {
        let mut t = Template::new();
        t.quad_facing(
            [
                vec3(-5., 0., -50.),
                vec3(5., 0., -50.),
                vec3(5., 0., 50.),
                vec3(-5., 0., 50.),
            ],
            Vec3::Y,
            C,
            0.,
        );
        // A kerb stripe lying on the road at its edge: inside the road quad.
        t.quad_facing(
            [
                vec3(4., kerb_y, -10.),
                vec3(5., kerb_y, -10.),
                vec3(5., kerb_y, 10.),
                vec3(4., kerb_y, 10.),
            ],
            Vec3::Y,
            [1., 0., 0.],
            0.,
        );
        t
    }

    #[test]
    fn the_spooky_kart_kerb_coplanar_inside_the_road_quad_is_found() {
        let defects = lint(&road_with_kerb(0.));
        let overlaps: Vec<_> = defects
            .iter()
            .filter(|d| matches!(d, Defect::CoplanarOverlap { .. }))
            .collect();
        assert_eq!(
            overlaps.len(),
            1,
            "one finding per pair of parts: {defects:?}"
        );
        match overlaps[0] {
            Defect::CoplanarOverlap { area, .. } => {
                assert!(
                    (area - 20.).abs() < 0.01,
                    "the whole stripe, 1 x 20, overlaps: {area}"
                )
            }
            _ => unreachable!(),
        }
        assert!(defects.len() == 1);
        assert!(defects[0].to_string().contains("z-fighting"));
    }

    #[test]
    fn lifting_the_kerb_clear_of_the_road_clears_the_finding_until_the_strict_epsilon_says_otherwise(
    ) {
        assert!(lint(&road_with_kerb(0.05)).is_empty());
        // 5 cm is below what a 0.1 near plane resolves at 300 m, and the strict tolerance knows it.
        let strict = LintConfig {
            plane_epsilon: depth_resolution(0.1, 300.),
            ..Default::default()
        };
        assert!(strict.plane_epsilon > 0.05);
        assert!(!lint_with(&road_with_kerb(0.05), &strict).is_empty());
    }

    #[test]
    fn the_depth_resolution_follows_the_square_of_the_distance_and_the_near_plane() {
        let a = depth_resolution(0.1, 100.);
        assert!((a - 0.006).abs() < 0.0005, "{a}");
        assert!((depth_resolution(0.1, 200.) / a - 4.).abs() < 1e-3);
        assert!((depth_resolution(0.05, 100.) / a - 2.).abs() < 1e-3);
    }

    #[test]
    fn touching_tiles_back_to_back_planes_and_stacked_floors_are_not_z_fighting() {
        let mut t = Template::new();
        // A 10 x 10 floor of separate tiles sharing edges: area of overlap is zero.
        for i in 0..10 {
            for j in 0..10 {
                let (x, z) = (i as f32, j as f32);
                t.quad_facing(
                    [
                        vec3(x, 0., z),
                        vec3(x + 1., 0., z),
                        vec3(x + 1., 0., z + 1.),
                        vec3(x, 0., z + 1.),
                    ],
                    Vec3::Y,
                    C,
                    0.,
                );
            }
        }
        // A two-sided sheet: opposite-facing coplanar quads are culled one at a time, never fight.
        t.quad_facing(
            [
                vec3(0., 3., 0.),
                vec3(2., 3., 0.),
                vec3(2., 3., 2.),
                vec3(0., 3., 2.),
            ],
            Vec3::Y,
            C,
            0.,
        );
        t.quad_facing(
            [
                vec3(0., 3., 0.),
                vec3(2., 3., 0.),
                vec3(2., 3., 2.),
                vec3(0., 3., 2.),
            ],
            -Vec3::Y,
            C,
            0.,
        );
        // A box standing on the floor: its bottom and the floor face opposite ways.
        t.box_(vec3(5., 0.5, 5.), vec3(0.5, 0.5, 0.5), C, 0.);
        assert!(lint(&t).is_empty(), "{:?}", lint(&t));
    }

    #[test]
    fn two_boxes_sharing_a_top_plane_do_fight_and_the_report_is_sorted_and_capped() {
        let mut t = Template::new();
        t.box_(Vec3::ZERO, vec3(1., 1., 1.), C, 0.);
        t.box_(vec3(0.5, 0., 0.), vec3(1., 1., 1.), C, 0.);
        let d = lint(&t);
        assert!(
            d.iter()
                .any(|d| matches!(d, Defect::CoplanarOverlap { .. })),
            "{d:?}"
        );
        // The cap: ask for at most 2 findings from a template with many.
        let mut many = Template::new();
        for i in 0..20 {
            many.quad_facing(
                [
                    vec3(0., 0., 0.),
                    vec3(1., 0., 0.),
                    vec3(1., 0., 1.),
                    vec3(0., 0., 1.),
                ],
                Vec3::Y,
                [i as f32 / 20., 0., 0.],
                0.,
            );
        }
        let capped = lint_with(
            &many,
            &LintConfig {
                max_defects: 2,
                ..Default::default()
            },
        );
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[2], Defect::Truncated { shown: 2 });
        // And the work cap.
        let worked = lint_with(
            &many,
            &LintConfig {
                max_pair_tests: 3,
                ..Default::default()
            },
        );
        assert!(matches!(worked.last(), Some(Defect::Truncated { .. })));
    }

    #[test]
    fn zero_area_winding_and_bad_index_defects() {
        let mut t = Template::new();
        // A real triangle wound against its normals (+Y normals, clockwise from above).
        t.quad(
            [
                vec3(0., 0., 0.),
                vec3(1., 0., 0.),
                vec3(1., 0., 1.),
                vec3(0., 0., 1.),
            ],
            Vec3::Y,
            C,
            0.,
        );
        assert!(lint(&t)
            .iter()
            .any(|d| matches!(d, Defect::WindingMismatch { .. })));
        // quad_facing fixes exactly that.
        let mut ok = Template::new();
        ok.quad_facing(
            [
                vec3(0., 0., 0.),
                vec3(1., 0., 0.),
                vec3(1., 0., 1.),
                vec3(0., 0., 1.),
            ],
            Vec3::Y,
            C,
            0.,
        );
        assert!(lint(&ok).is_empty());
        // Three collinear points.
        let mut z = Template::new();
        z.quad(
            [
                vec3(0., 0., 0.),
                vec3(1., 0., 0.),
                vec3(2., 0., 0.),
                vec3(3., 0., 0.),
            ],
            Vec3::Y,
            C,
            0.,
        );
        let d = lint(&z);
        assert_eq!(
            d.iter()
                .filter(|d| matches!(d, Defect::ZeroArea { .. }))
                .count(),
            2,
            "{d:?}"
        );
        // An index past the end of the vertex list, and a dangling index.
        let mut b = ok.clone();
        b.idx.extend_from_slice(&[0, 1, 99]);
        assert!(lint(&b).contains(&Defect::BadIndex { tri: 2 }));
        b.idx.push(0);
        assert!(lint(&b).contains(&Defect::BadIndex { tri: 3 }));
    }

    #[test]
    fn the_kit_primitives_and_the_shape_toolkit_are_clean() {
        use crate::viewer::kit::shape::{Ring, Section, Sweep};
        let mut t = Template::new();
        t.box_(Vec3::ZERO, vec3(0.5, 1., 1.5), C, 0.);
        assert_clean(&t, "box");
        for (what, build) in [
            (
                "ball",
                Box::new(|t: &mut Template| t.ball(Vec3::ZERO, vec3(1., 2., 1.), C, 0., 12, 8))
                    as Box<dyn Fn(&mut Template)>,
            ),
            (
                "cone",
                Box::new(|t| t.cone(Vec3::ZERO, 1., 0., 2., C, 0., 12)),
            ),
            (
                "frustum",
                Box::new(|t| t.cone(Vec3::ZERO, 1., 0.5, 2., C, 0., 12)),
            ),
            (
                "cylinder",
                Box::new(|t| t.cylinder(Vec3::ZERO, 1., 2., C, 0., 12)),
            ),
            ("disc", Box::new(|t| t.disc(Vec3::ZERO, 1., C, 0., 12))),
            ("ring", Box::new(|t| t.ring(Vec3::ZERO, 0.5, 1., C, 0., 12))),
            (
                "soft_ring",
                Box::new(|t| t.soft_ring(Vec3::ZERO, 0.5, 1., C, 1., C, 0., 0., 12)),
            ),
            (
                "tube",
                Box::new(|t| t.tube(Vec3::ZERO, 1., 2., C, 1., 0., 0., 12)),
            ),
            (
                "rounded_box",
                Box::new(|t| t.rounded_box(Vec3::ZERO, vec3(0.5, 1., 1.5), 0.2, C, 0.)),
            ),
            (
                "capsule",
                Box::new(|t| t.capsule(Vec3::ZERO, vec3(0., 2., 1.), 0.3, C, 0., 12)),
            ),
            (
                "rod",
                Box::new(|t| t.rod(Vec3::ZERO, vec3(1., 2., 1.), 0.3, 0.1, C, 0., 10)),
            ),
            (
                "loft",
                Box::new(|t| {
                    let secs = [
                        Section::across_z(0., 0., 0., 0.2, 0.3, 0.05),
                        Section::across_z(1., 0., 0.1, 0.25, 0.2, 0.1),
                        Section::across_z(2., 0., 0., 0.1, 0.1, 0.05),
                    ];
                    t.loft(&secs, Ring::Rounded(2), true, C, 0.)
                }),
            ),
            (
                "sweep",
                Box::new(|t| {
                    let p = crate::viewer::kit::shape::arc(
                        Vec3::ZERO,
                        Vec3::X,
                        Vec3::Y,
                        [1., 1.],
                        0.,
                        2.,
                        10,
                    );
                    t.sweep(&p, &Sweep::strap(Vec3::Z, 0.1, 0.02), C, 0.)
                }),
            ),
        ] {
            let mut t = Template::new();
            build(&mut t);
            assert!(!t.is_empty(), "{what} built nothing");
            assert_clean(&t, what);
            let mirrored = t.mirrored_x();
            assert_clean(&mirrored, &format!("{what} mirrored"));
        }
        // sky_dome faces inwards on purpose and its normals agree.
        let mut sky = Template::new();
        sky.sky_dome(10., |_| C, 12, 6);
        assert_clean(&sky, "sky dome");
    }

    fn bend() -> Vec<Vec3> {
        // Half a circle of radius 3 in 15 degree steps: a bend tighter than the strip is wide.
        (0..=12)
            .map(|i| {
                let a = (15. * i as f32).to_radians();
                vec3(3. * a.cos(), 0., -3. * a.sin())
            })
            .collect()
    }

    /// Offset each sample along its own normal with no mitre and no clamp: the bug that shipped.
    fn naive_strip(path: &[Vec3], half: f32) -> Vec<Quad> {
        let n = path.len();
        let side = |i: usize, s: f32| {
            let (p, q) = (path[i.saturating_sub(1)], path[(i + 1).min(n - 1)]);
            let t = vec3(q.x - p.x, 0., q.z - p.z).normalize_or_zero();
            path[i] + vec3(-t.z, 0., t.x) * half * s
        };
        (0..n - 1)
            .map(|i| [side(i, -1.), side(i, 1.), side(i + 1, 1.), side(i + 1, -1.)])
            .collect()
    }

    #[test]
    fn a_naive_normal_offset_round_a_tight_bend_folds_and_offset_strip_does_not() {
        let path = bend();
        let half = 8.; // wider than the 3 m radius of the bend
        let naive = naive_strip(&path, half);
        let folds = strip_folds(&naive);
        assert!(!folds.is_empty(), "the naive strip must trip the check");
        let mut bad = Template::new();
        for q in &naive {
            bad.quad_facing(*q, Vec3::Y, C, 0.);
        }
        assert!(
            lint(&bad)
                .iter()
                .any(|d| matches!(d, Defect::CoplanarOverlap { .. })),
            "a folded strip also overlaps itself: {:?}",
            lint(&bad)
        );

        let opts = StripOpts::default();
        assert!(strip_folds(&strip_quads(&path, false, -half, half, &opts)).is_empty());
        let mut good = Template::new();
        good.offset_strip(&path, false, half, &opts, C, 0.);
        assert!(!good.is_empty());
        assert_clean(&good, "offset strip");
    }

    #[test]
    fn the_strip_check_catches_a_bow_tie_a_reversed_quad_and_a_flipped_neighbour() {
        let ok: Quad = [
            vec3(-1., 0., 0.),
            vec3(1., 0., 0.),
            vec3(1., 0., -2.),
            vec3(-1., 0., -2.),
        ];
        assert!(strip_folds(&[ok]).is_empty());
        // Bow tie: the far corners swapped.
        let bow: Quad = [ok[0], ok[1], ok[3], ok[2]];
        assert_eq!(strip_folds(&[bow]), vec![Defect::StripFold { quad: 0 }]);
        // A quad turned over relative to its neighbour.
        let flipped: Quad = [ok[1], ok[0], ok[3], ok[2]];
        assert_eq!(
            strip_folds(&[ok, flipped]),
            vec![Defect::StripFold { quad: 1 }]
        );
        // A zero-area quad is not a fold.
        let point: Quad = [Vec3::ZERO; 4];
        assert!(strip_folds(&[ok, point]).is_empty());
    }

    #[test]
    fn a_big_template_stays_fast_enough_to_lint_in_a_test() {
        // 90 000 triangles of floor: tens of thousands of candidate pairs per column, all touching only
        // along edges.
        let mut t = Template::new();
        for i in 0..150 {
            for j in 0..150 {
                let (x, z) = (i as f32, j as f32);
                t.quad_facing(
                    [
                        vec3(x, 0., z),
                        vec3(x + 1., 0., z),
                        vec3(x + 1., 0., z + 1.),
                        vec3(x, 0., z + 1.),
                    ],
                    Vec3::Y,
                    C,
                    0.,
                );
            }
            if t.verts.len() > 60_000 {
                break;
            }
        }
        let start = std::time::Instant::now();
        assert!(lint(&t).is_empty());
        assert!(start.elapsed().as_secs_f32() < 20., "{:?}", start.elapsed());
    }
}
