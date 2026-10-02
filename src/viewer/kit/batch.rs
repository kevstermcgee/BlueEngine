//! Procedural geometry and per-frame dynamic batching.
//!
//! The engine bakes *static* worlds once (`mesh::bake_with`). Everything that moves or appears at
//! runtime (enemies, pickups, particles, beams, a whole procedural arena that changes) needs the other
//! half: build a few small [`Template`]s once (boxes, balls, cones, rings, tubes), then every frame
//! place them with a transform and a [`Tint`] into a [`Batch`], which packs them on the CPU into a
//! handful of large macroquad meshes, so a frame of hundreds of actors costs a few draw calls.
//!
//! Vertex attributes are packed for the kit's own shaders ([`Materials`](super::Materials)):
//! `normal.xyz` is the surface normal, `uv.x` is how much the surface glows on its own, `color` is
//! RGBA8. No textures are involved.
use macroquad::prelude::*;

/// Largest vertex and index count of one mesh a [`Batch`] builds. They stay below the 30 000-entry
/// draw-call capacities of `game_client::window_config`; raise both in your `Conf` if you raise these.
pub const MAX_MESH_VERTICES: usize = 9_000;
/// See [`MAX_MESH_VERTICES`].
pub const MAX_MESH_INDICES: usize = 27_000;

/// A linear RGB colour, each channel 0-1.
pub type Rgb = [f32; 3];

/// One template vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vert {
    /// Position in the template's local space.
    pub p: Vec3,
    /// Unit surface normal.
    pub n: Vec3,
    /// Base colour.
    pub c: Rgb,
    /// Self-illumination: 0 = lit by the scene only, 1 = glows with its own colour.
    pub e: f32,
    /// Opacity, 0-1.
    pub a: f32,
}

/// A small indexed mesh in local space, built with the primitive methods and placed with
/// [`Batch::add`]. Indices are `u16`, so one template holds at most 65 535 vertices.
#[derive(Clone, Debug)]
pub struct Template {
    /// Vertices.
    pub verts: Vec<Vert>,
    /// Triangle list indices into `verts`.
    pub idx: Vec<u16>,
    /// Opacity given to vertices pushed from now on (1 = opaque).
    pub alpha: f32,
}

impl Default for Template {
    fn default() -> Self {
        Self {
            verts: Vec::new(),
            idx: Vec::new(),
            alpha: 1.,
        }
    }
}

impl Template {
    /// An empty template.
    pub fn new() -> Self {
        Self::default()
    }
    /// True when nothing has been added.
    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    fn push(&mut self, p: Vec3, n: Vec3, c: Rgb, e: f32) -> u16 {
        // Indices are u16: past 65 535 vertices they would wrap and corrupt the mesh, so refuse loudly.
        let i = u16::try_from(self.verts.len()).expect(
            "a Template holds at most 65 535 vertices: build big scenes from several templates",
        );
        let a = self.alpha;
        self.verts.push(Vert { p, n, c, e, a });
        i
    }

    /// Filled disc lying in the XZ plane (normal +Y).
    pub fn disc(&mut self, center: Vec3, r: f32, c: Rgb, e: f32, sides: usize) {
        let hub = self.push(center, Vec3::Y, c, e);
        let first = self.verts.len() as u16;
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            self.push(center + vec3(a.cos() * r, 0., a.sin() * r), Vec3::Y, c, e);
        }
        for i in 0..sides as u16 {
            self.idx.extend_from_slice(&[hub, first + i + 1, first + i]);
        }
    }

    /// A ring whose outer edge has its own colour and alpha (soft halos, glows).
    #[allow(clippy::too_many_arguments)]
    pub fn soft_ring(
        &mut self,
        center: Vec3,
        r_in: f32,
        r_out: f32,
        c_in: Rgb,
        a_in: f32,
        c_out: Rgb,
        a_out: f32,
        e: f32,
        sides: usize,
    ) {
        let base = self.verts.len() as u16;
        let old = self.alpha;
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let (s, co) = a.sin_cos();
            self.alpha = a_in;
            self.push(center + vec3(co * r_in, 0., s * r_in), Vec3::Y, c_in, e);
            self.alpha = a_out;
            self.push(center + vec3(co * r_out, 0., s * r_out), Vec3::Y, c_out, e);
        }
        self.alpha = old;
        for i in 0..sides as u16 {
            let a = base + i * 2;
            // Counter-clockwise seen from above, like the +Y normals.
            self.idx
                .extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }

    /// One flat quad; corners in counter-clockwise order seen from the front.
    pub fn quad(&mut self, q: [Vec3; 4], n: Vec3, c: Rgb, e: f32) {
        let a = self.push(q[0], n, c, e);
        let b = self.push(q[1], n, c, e);
        let cc = self.push(q[2], n, c, e);
        let d = self.push(q[3], n, c, e);
        self.idx.extend_from_slice(&[a, b, cc, a, cc, d]);
    }

    /// A flat quad that faces the way `normal` says, whatever order the corners are given in. Use this
    /// for ground, walls and roads; [`Template::quad`] silently culls a quad wound the wrong way.
    pub fn quad_facing(&mut self, q: [Vec3; 4], normal: Vec3, c: Rgb, e: f32) {
        let geometric = (q[1] - q[0]).cross(q[2] - q[0]);
        if geometric.dot(normal) >= 0. {
            self.quad(q, normal, c, e);
        } else {
            self.quad([q[3], q[2], q[1], q[0]], normal, c, e);
        }
    }

    /// Axis-aligned box: `half` is the half-extent on each axis.
    pub fn box_(&mut self, center: Vec3, half: Vec3, c: Rgb, e: f32) {
        self.box_faces(center, half, c, e, c, e);
    }

    /// A box with a different colour (and glow) on its top face: floor tiles, pillars, crates. The sides
    /// never glow.
    pub fn box_top(&mut self, center: Vec3, half: Vec3, side: Rgb, top: Rgb, top_glow: f32) {
        self.box_faces(center, half, side, 0., top, top_glow);
    }

    fn box_faces(
        &mut self,
        center: Vec3,
        half: Vec3,
        side: Rgb,
        side_glow: f32,
        top: Rgb,
        top_glow: f32,
    ) {
        let v = |sx: f32, sy: f32, sz: f32| center + vec3(sx * half.x, sy * half.y, sz * half.z);
        self.quad(
            [
                v(1., -1., 1.),
                v(1., -1., -1.),
                v(1., 1., -1.),
                v(1., 1., 1.),
            ],
            Vec3::X,
            side,
            side_glow,
        );
        self.quad(
            [
                v(-1., -1., -1.),
                v(-1., -1., 1.),
                v(-1., 1., 1.),
                v(-1., 1., -1.),
            ],
            -Vec3::X,
            side,
            side_glow,
        );
        self.quad(
            [
                v(-1., 1., 1.),
                v(1., 1., 1.),
                v(1., 1., -1.),
                v(-1., 1., -1.),
            ],
            Vec3::Y,
            top,
            top_glow,
        );
        self.quad(
            [
                v(-1., -1., -1.),
                v(1., -1., -1.),
                v(1., -1., 1.),
                v(-1., -1., 1.),
            ],
            -Vec3::Y,
            side,
            side_glow,
        );
        self.quad(
            [
                v(-1., -1., 1.),
                v(1., -1., 1.),
                v(1., 1., 1.),
                v(-1., 1., 1.),
            ],
            Vec3::Z,
            side,
            side_glow,
        );
        self.quad(
            [
                v(1., -1., -1.),
                v(-1., -1., -1.),
                v(-1., 1., -1.),
                v(1., 1., -1.),
            ],
            -Vec3::Z,
            side,
            side_glow,
        );
    }

    /// Ellipsoid with per-axis radii.
    pub fn ball(&mut self, center: Vec3, radii: Vec3, c: Rgb, e: f32, segs: usize, rings: usize) {
        let base = self.verts.len() as u16;
        for j in 0..=rings {
            let a = j as f32 / rings as f32 * std::f32::consts::PI;
            for i in 0..=segs {
                let b = i as f32 / segs as f32 * std::f32::consts::TAU;
                let dir = vec3(a.sin() * b.cos(), a.cos(), a.sin() * b.sin());
                let n = vec3(dir.x / radii.x, dir.y / radii.y, dir.z / radii.z).normalize_or_zero();
                self.push(center + dir * radii, n, c, e);
            }
        }
        let w = segs as u16 + 1;
        for j in 0..rings as u16 {
            for i in 0..segs as u16 {
                let a = base + j * w + i;
                // Counter-clockwise seen from outside, like the normals (`lint` checks the agreement).
                self.idx
                    .extend_from_slice(&[a, a + 1, a + w, a + 1, a + w + 1, a + w]);
            }
        }
    }

    /// Cone or frustum standing on `base` (a point on its bottom disc), pointing +Y, with end caps.
    #[allow(clippy::too_many_arguments)]
    pub fn cone(
        &mut self,
        base: Vec3,
        r_bottom: f32,
        r_top: f32,
        height: f32,
        c: Rgb,
        e: f32,
        sides: usize,
    ) {
        let slope = (r_bottom - r_top) / height.max(1e-4);
        let start = self.verts.len() as u16;
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let (s, co) = a.sin_cos();
            let n = vec3(co, slope, s).normalize_or_zero();
            self.push(base + vec3(co * r_bottom, 0., s * r_bottom), n, c, e);
            self.push(base + vec3(co * r_top, height, s * r_top), n, c, e);
        }
        for i in 0..sides as u16 {
            let a = start + i * 2;
            self.idx
                .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        if r_top > 0.001 {
            let centre = self.push(base + vec3(0., height, 0.), Vec3::Y, c, e);
            let first = self.verts.len() as u16;
            for i in 0..=sides {
                let a = i as f32 / sides as f32 * std::f32::consts::TAU;
                self.push(
                    base + vec3(a.cos() * r_top, height, a.sin() * r_top),
                    Vec3::Y,
                    c,
                    e,
                );
            }
            for i in 0..sides as u16 {
                self.idx
                    .extend_from_slice(&[centre, first + i + 1, first + i]);
            }
        }
        if r_bottom > 0.001 {
            let centre = self.push(base, -Vec3::Y, c, e);
            let first = self.verts.len() as u16;
            for i in 0..=sides {
                let a = i as f32 / sides as f32 * std::f32::consts::TAU;
                self.push(
                    base + vec3(a.cos() * r_bottom, 0., a.sin() * r_bottom),
                    -Vec3::Y,
                    c,
                    e,
                );
            }
            for i in 0..sides as u16 {
                self.idx
                    .extend_from_slice(&[centre, first + i, first + i + 1]);
            }
        }
    }

    /// A capped cylinder standing on `base`.
    pub fn cylinder(&mut self, base: Vec3, r: f32, height: f32, c: Rgb, e: f32, sides: usize) {
        self.cone(base, r, r, height, c, e, sides);
    }

    /// A flat ring lying in the XZ plane (normal +Y): shockwaves and target markers.
    pub fn ring(&mut self, center: Vec3, r_inner: f32, r_outer: f32, c: Rgb, e: f32, sides: usize) {
        let base = self.verts.len() as u16;
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let (s, co) = a.sin_cos();
            self.push(center + vec3(co * r_inner, 0., s * r_inner), Vec3::Y, c, e);
            self.push(center + vec3(co * r_outer, 0., s * r_outer), Vec3::Y, c, e);
        }
        for i in 0..sides as u16 {
            let a = base + i * 2;
            // Counter-clockwise seen from above, like the +Y normals.
            self.idx
                .extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }

    /// An open cylinder wall standing on `base` (no caps): shockwave curtains and light beams. Alpha
    /// runs from `a_bottom` at the base to `a_top` at the rim.
    #[allow(clippy::too_many_arguments)]
    pub fn tube(
        &mut self,
        base: Vec3,
        r: f32,
        height: f32,
        c: Rgb,
        a_bottom: f32,
        a_top: f32,
        e: f32,
        sides: usize,
    ) {
        let start = self.verts.len() as u16;
        let old = self.alpha;
        for i in 0..=sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let (s, co) = a.sin_cos();
            let n = vec3(co, 0., s);
            self.alpha = a_bottom;
            self.push(base + vec3(co * r, 0., s * r), n, c, e);
            self.alpha = a_top;
            self.push(base + vec3(co * r, height, s * r), n, c, e);
        }
        self.alpha = old;
        for i in 0..sides as u16 {
            let a = start + i * 2;
            self.idx
                .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }

    /// A sky dome of `radius` around the origin, coloured by `gradient(elevation)` where elevation is
    /// the sine of the angle above the horizon (-1 straight down, 0 horizon, 1 zenith). Draw it with the
    /// sky material and [`View::sky_camera`](super::View::sky_camera).
    pub fn sky_dome(
        &mut self,
        radius: f32,
        gradient: impl Fn(f32) -> Rgb,
        segs: usize,
        rings: usize,
    ) {
        let base = self.verts.len() as u16;
        for j in 0..=rings {
            let a = j as f32 / rings as f32 * std::f32::consts::PI;
            for i in 0..=segs {
                let b = i as f32 / segs as f32 * std::f32::consts::TAU;
                let dir = vec3(a.sin() * b.cos(), a.cos(), a.sin() * b.sin());
                self.push(dir * radius, -dir, gradient(dir.y), 0.);
            }
        }
        let w = segs as u16 + 1;
        for j in 0..rings as u16 {
            for i in 0..segs as u16 {
                let a = base + j * w + i;
                // Wound to face inwards: the camera sits inside.
                self.idx
                    .extend_from_slice(&[a, a + w, a + 1, a + 1, a + w, a + w + 1]);
            }
        }
    }

    /// Merge another template already in the same local space.
    pub fn append(&mut self, other: &Template) {
        let base = self.verts.len() as u16;
        self.verts.extend_from_slice(&other.verts);
        self.idx.extend(other.idx.iter().map(|i| i + base));
    }

    /// A copy with every position moved by `m` and every normal rotated by it.
    pub fn transformed(&self, m: Mat4) -> Template {
        Template {
            verts: self
                .verts
                .iter()
                .map(|v| Vert {
                    p: m.transform_point3(v.p),
                    n: m.transform_vector3(v.n).normalize_or_zero(),
                    ..*v
                })
                .collect(),
            idx: self.idx.clone(),
            alpha: self.alpha,
        }
    }

    /// Split into templates that each fit one mesh ([`MAX_MESH_VERTICES`], [`MAX_MESH_INDICES`]), keeping
    /// every triangle. A template that already fits comes back as the only element.
    pub fn split(&self) -> Vec<Template> {
        if self.verts.len() <= MAX_MESH_VERTICES && self.idx.len() <= MAX_MESH_INDICES {
            return vec![self.clone()];
        }
        let mut parts = Vec::new();
        let mut part = Template {
            alpha: self.alpha,
            ..Template::default()
        };
        let mut remap: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();
        for tri in self.idx.chunks_exact(3) {
            let new_verts = tri.iter().filter(|i| !remap.contains_key(i)).count();
            if part.verts.len() + new_verts > MAX_MESH_VERTICES
                || part.idx.len() + 3 > MAX_MESH_INDICES
            {
                parts.push(std::mem::replace(
                    &mut part,
                    Template {
                        alpha: self.alpha,
                        ..Template::default()
                    },
                ));
                remap.clear();
            }
            for &i in tri {
                let mapped = *remap.entry(i).or_insert_with(|| {
                    part.verts.push(self.verts[i as usize]);
                    (part.verts.len() - 1) as u16
                });
                part.idx.push(mapped);
            }
        }
        if !part.idx.is_empty() {
            parts.push(part);
        }
        parts
    }

    /// Bake into a static set of macroquad meshes, splitting to stay inside draw-call limits (a template
    /// bigger than one mesh is split, never dropped).
    pub fn to_meshes(&self) -> Vec<Mesh> {
        let mut batch = Batch::new();
        batch.add(self, Mat4::IDENTITY, Tint::NONE);
        batch.meshes.retain(|m| !m.vertices.is_empty());
        batch.meshes
    }
}

/// Per-instance colour change applied when a template is placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    /// Multiplies the template colour.
    pub mul: Rgb,
    /// Added afterwards (hit flashes).
    pub add: Rgb,
    /// Multiplies the template's opacity.
    pub alpha: f32,
    /// Multiplies the template's self-illumination.
    pub glow: f32,
}

impl Tint {
    /// Leave the template as built.
    pub const NONE: Tint = Tint {
        mul: [1., 1., 1.],
        add: [0., 0., 0.],
        alpha: 1.,
        glow: 1.,
    };
    /// Only change opacity.
    pub fn alpha(a: f32) -> Self {
        Self {
            alpha: a,
            ..Self::NONE
        }
    }
    /// Add light to every colour channel: a white hit flash is `Tint::flash(0.5)`.
    pub fn flash(amount: f32) -> Self {
        Self {
            add: [amount; 3],
            ..Self::NONE
        }
    }
}

fn to_u8(x: f32) -> u8 {
    (x.clamp(0., 1.) * 255.).round() as u8
}

/// Dynamic geometry for one frame: [`Batch::clear`], place things with [`Batch::add`],
/// [`Batch::billboard`] and [`Batch::beam`], then [`Batch::draw`] with the material bound. Reuses its
/// buffers, so the steady state allocates nothing.
pub struct Batch {
    /// The packed meshes (only the non-empty ones are drawn).
    pub meshes: Vec<Mesh>,
    cur: usize,
}

fn empty_mesh() -> Mesh {
    Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        texture: None,
    }
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}

impl Batch {
    /// An empty batch.
    pub fn new() -> Self {
        Self {
            meshes: vec![empty_mesh()],
            cur: 0,
        }
    }
    /// Forget last frame's geometry, keeping the allocations.
    pub fn clear(&mut self) {
        for m in &mut self.meshes {
            m.vertices.clear();
            m.indices.clear();
        }
        self.cur = 0;
    }
    /// Vertices placed so far, across all meshes.
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    fn room_for(&mut self, verts: usize, indices: usize) {
        let full = {
            let m = &self.meshes[self.cur];
            m.vertices.len() + verts > MAX_MESH_VERTICES
                || m.indices.len() + indices > MAX_MESH_INDICES
        };
        if full {
            self.cur += 1;
            if self.cur == self.meshes.len() {
                self.meshes.push(empty_mesh());
            }
        }
    }

    /// Place a template in world space with transform `m`. A template too large for one mesh is split
    /// into several rather than overflowing the draw call (it used to be dropped without a word).
    pub fn add(&mut self, t: &Template, m: Mat4, tint: Tint) {
        if t.verts.is_empty() {
            return;
        }
        if t.verts.len() > MAX_MESH_VERTICES || t.idx.len() > MAX_MESH_INDICES {
            for part in t.split() {
                self.add(&part, m, tint);
            }
            return;
        }
        self.room_for(t.verts.len(), t.idx.len());
        let mesh = &mut self.meshes[self.cur];
        let base = mesh.vertices.len() as u16;
        for v in &t.verts {
            let p = m.transform_point3(v.p);
            let n = m.transform_vector3(v.n).normalize_or_zero();
            let col = [
                to_u8(v.c[0] * tint.mul[0] + tint.add[0]),
                to_u8(v.c[1] * tint.mul[1] + tint.add[1]),
                to_u8(v.c[2] * tint.mul[2] + tint.add[2]),
                to_u8(tint.alpha * v.a),
            ];
            let mut vert = Vertex::new2(p, vec2(v.e * tint.glow, 0.), WHITE);
            vert.color = col;
            vert.normal = vec4(n.x, n.y, n.z, 0.);
            mesh.vertices.push(vert);
        }
        mesh.indices.extend(t.idx.iter().map(|i| i + base));
    }

    /// A camera-facing quad of `w` x `h` centred on `center`, spanned by the camera's `right` and `up`
    /// (take them from [`View`](super::View)): particles, sparks, glows.
    #[allow(clippy::too_many_arguments)]
    pub fn billboard(
        &mut self,
        center: Vec3,
        right: Vec3,
        up: Vec3,
        w: f32,
        h: f32,
        c: Rgb,
        alpha: f32,
        glow: f32,
    ) {
        self.room_for(4, 6);
        let mesh = &mut self.meshes[self.cur];
        let base = mesh.vertices.len() as u16;
        let n = right.cross(up).normalize_or_zero();
        let col = [to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(alpha)];
        for (sx, sy) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
            let p = center + right * (sx * w * 0.5) + up * (sy * h * 0.5);
            let mut v = Vertex::new2(p, vec2(glow, 0.), WHITE);
            v.color = col;
            v.normal = vec4(n.x, n.y, n.z, 0.);
            mesh.vertices.push(v);
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A ribbon between two points that faces `eye`, `width` wide, fading from `alpha_a` to `alpha_b`:
    /// trails, tracers, beams.
    #[allow(clippy::too_many_arguments)]
    pub fn beam(
        &mut self,
        a: Vec3,
        b: Vec3,
        eye: Vec3,
        width: f32,
        c: Rgb,
        alpha_a: f32,
        alpha_b: f32,
        glow: f32,
    ) {
        self.room_for(4, 6);
        let axis = b - a;
        let to_eye = eye - (a + b) * 0.5;
        let side = axis.cross(to_eye).normalize_or_zero() * (width * 0.5);
        let mesh = &mut self.meshes[self.cur];
        let base = mesh.vertices.len() as u16;
        let n = to_eye.normalize_or_zero();
        for (p, al) in [
            (a - side, alpha_a),
            (a + side, alpha_a),
            (b + side, alpha_b),
            (b - side, alpha_b),
        ] {
            let mut v = Vertex::new2(p, vec2(glow, 0.), WHITE);
            v.color = [to_u8(c[0]), to_u8(c[1]), to_u8(c[2]), to_u8(al)];
            v.normal = vec4(n.x, n.y, n.z, 0.);
            mesh.vertices.push(v);
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Draw every non-empty mesh with whatever material is currently bound.
    pub fn draw(&self) {
        for m in &self.meshes {
            if !m.vertices.is_empty() {
                draw_mesh(m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_has_six_quads_with_outward_normals_and_counter_clockwise_winding() {
        let mut t = Template::new();
        t.box_(vec3(1., 2., 3.), vec3(0.5, 1., 1.5), [1., 0., 0.], 0.);
        assert_eq!((t.verts.len(), t.idx.len()), (24, 36));
        for tri in t.idx.chunks(3) {
            let (a, b, c) = (
                t.verts[tri[0] as usize],
                t.verts[tri[1] as usize],
                t.verts[tri[2] as usize],
            );
            let geometric = (b.p - a.p).cross(c.p - a.p).normalize();
            assert!(
                geometric.dot(a.n) > 0.99,
                "winding disagrees with the stored normal"
            );
            assert!(
                (a.p - vec3(1., 2., 3.)).dot(a.n) > 0.,
                "normal points inward"
            );
        }
    }

    #[test]
    fn quad_facing_faces_the_given_normal_whichever_way_the_corners_are_ordered() {
        let square = [
            vec3(0., 0., 0.),
            vec3(0., 0., 1.),
            vec3(1., 0., 1.),
            vec3(1., 0., 0.),
        ];
        let mut reversed = square;
        reversed.reverse();
        for corners in [square, reversed] {
            for normal in [Vec3::Y, -Vec3::Y] {
                let mut t = Template::new();
                t.quad_facing(corners, normal, [1.; 3], 0.);
                for tri in t.idx.chunks(3) {
                    let (a, b, c) = (
                        t.verts[tri[0] as usize],
                        t.verts[tri[1] as usize],
                        t.verts[tri[2] as usize],
                    );
                    assert!((b.p - a.p).cross(c.p - a.p).normalize().dot(normal) > 0.99);
                }
            }
        }
    }

    fn big_template(quads: usize) -> Template {
        let mut t = Template::new();
        for i in 0..quads {
            let x = i as f32;
            t.quad_facing(
                [
                    vec3(x, 0., 0.),
                    vec3(x + 1., 0., 0.),
                    vec3(x + 1., 0., 1.),
                    vec3(x, 0., 1.),
                ],
                Vec3::Y,
                [x / quads as f32, 0.5, 0.5],
                0.,
            );
        }
        t
    }

    #[test]
    fn a_template_over_the_mesh_limit_is_split_not_dropped() {
        // 4 vertices a quad: 6 000 quads is 24 000 vertices, well over the 9 000 limit.
        let t = big_template(6_000);
        assert!(t.verts.len() > MAX_MESH_VERTICES);
        let parts = t.split();
        assert!(parts.len() >= 3, "{} parts", parts.len());
        assert_eq!(
            parts.iter().map(|p| p.idx.len()).sum::<usize>(),
            t.idx.len(),
            "no triangle lost"
        );
        assert!(parts
            .iter()
            .all(|p| p.verts.len() <= MAX_MESH_VERTICES && p.idx.len() <= MAX_MESH_INDICES));
        for p in &parts {
            assert!(
                p.idx.iter().all(|i| (*i as usize) < p.verts.len()),
                "indices stay inside their part"
            );
        }
        let meshes = t.to_meshes();
        assert_eq!(
            meshes.iter().map(|m| m.indices.len()).sum::<usize>(),
            t.idx.len()
        );
        assert!(meshes.iter().all(|m| m.vertices.len() <= MAX_MESH_VERTICES));
    }

    #[test]
    fn a_template_that_fits_is_left_whole() {
        let t = big_template(100);
        let parts = t.split();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].verts.len(), t.verts.len());
    }

    #[test]
    #[should_panic(expected = "65 535 vertices")]
    fn a_template_refuses_to_wrap_its_indices() {
        let mut t = Template::new();
        for i in 0..20_000 {
            let x = i as f32;
            t.quad(
                [
                    vec3(x, 0., 0.),
                    vec3(x, 0., 1.),
                    vec3(x + 1., 0., 1.),
                    vec3(x + 1., 0., 0.),
                ],
                Vec3::Y,
                [1.; 3],
                0.,
            );
        }
    }

    #[test]
    fn box_top_colours_only_the_lid_and_glows_only_there() {
        let mut t = Template::new();
        t.box_top(Vec3::ZERO, Vec3::ONE, [0.1; 3], [1., 0.5, 0.], 1.);
        let lid: Vec<_> = t.verts.iter().filter(|v| v.n == Vec3::Y).collect();
        assert_eq!(lid.len(), 4);
        assert!(lid.iter().all(|v| v.c == [1., 0.5, 0.] && v.e == 1.));
        assert!(t
            .verts
            .iter()
            .filter(|v| v.n != Vec3::Y)
            .all(|v| v.c == [0.1; 3] && v.e == 0.));
    }

    #[test]
    fn primitives_index_in_range_with_unit_normals() {
        let mut t = Template::new();
        t.ball(
            vec3(0., 1., 0.),
            vec3(0.5, 1., 0.5),
            [0., 1., 0.],
            0.,
            12,
            8,
        );
        t.cone(vec3(0., 0., 0.), 1., 0., 2., [1.; 3], 0., 10);
        t.cylinder(vec3(3., 0., 0.), 0.5, 1., [1.; 3], 0., 8);
        t.ring(vec3(0., 0.1, 0.), 1., 1.2, [1.; 3], 1., 24);
        t.tube(Vec3::ZERO, 1., 2., [1.; 3], 1., 0., 1., 16);
        t.disc(Vec3::ZERO, 1., [1.; 3], 0., 12);
        t.soft_ring(Vec3::ZERO, 0.8, 1., [1.; 3], 0., [1.; 3], 1., 1., 20);
        assert!(t.idx.iter().all(|i| (*i as usize) < t.verts.len()));
        assert_eq!(t.idx.len() % 3, 0);
        assert!(t
            .verts
            .iter()
            .all(|v| (v.n.length() - 1.).abs() < 1e-3 || v.n == Vec3::ZERO));
        let top = t.verts[..117]
            .iter()
            .map(|v| v.p.y)
            .fold(f32::MIN, f32::max);
        assert!((top - 2.).abs() < 1e-3, "the ellipsoid's top is at y = 2");
    }

    #[test]
    fn tube_and_soft_ring_fade_between_their_edges() {
        let mut t = Template::new();
        t.tube(Vec3::ZERO, 1., 2., [1.; 3], 0.9, 0., 1., 8);
        assert!(t.verts.iter().any(|v| v.a == 0.9) && t.verts.iter().any(|v| v.a == 0.));
        assert_eq!(t.alpha, 1., "the template's own alpha is restored");
    }

    #[test]
    fn sky_dome_faces_inwards_and_follows_the_gradient() {
        let mut t = Template::new();
        t.sky_dome(100., |e| [e.max(0.), 0., 0.], 16, 8);
        assert!(t.verts.iter().all(|v| (v.p.length() - 100.).abs() < 1e-2));
        let top = t
            .verts
            .iter()
            .max_by(|a, b| a.p.y.total_cmp(&b.p.y))
            .unwrap();
        assert!(top.c[0] > 0.99);
        for tri in t
            .idx
            .chunks(3)
            .filter(|tri| tri[0] != tri[1] && tri[1] != tri[2])
        {
            let (a, b, c) = (
                t.verts[tri[0] as usize],
                t.verts[tri[1] as usize],
                t.verts[tri[2] as usize],
            );
            let geometric = (b.p - a.p).cross(c.p - a.p);
            if geometric.length() > 1e-3 {
                assert!(geometric.dot(-a.p) > 0., "triangle should face the centre");
            }
        }
    }

    #[test]
    fn batches_split_before_exceeding_draw_limits_and_clear_cleanly() {
        let mut t = Template::new();
        t.ball(Vec3::ZERO, Vec3::ONE, [1.; 3], 0., 16, 12);
        let mut b = Batch::new();
        for i in 0..200 {
            b.add(
                &t,
                Mat4::from_translation(vec3(i as f32, 0., 0.)),
                Tint::NONE,
            );
        }
        assert!(b.meshes.len() > 1, "200 balls need more than one mesh");
        for m in &b.meshes {
            assert!(m.vertices.len() <= MAX_MESH_VERTICES && m.indices.len() <= MAX_MESH_INDICES);
            assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
        }
        assert_eq!(b.vertex_count(), 200 * t.verts.len());
        b.clear();
        assert_eq!(b.vertex_count(), 0);
        b.add(&t, Mat4::IDENTITY, Tint::flash(0.5));
        assert_eq!(b.meshes[0].vertices.len(), t.verts.len());
        assert!(b.meshes[0].vertices[0].color[0] >= 127, "flash adds light");
    }

    #[test]
    fn an_oversized_template_is_split_across_meshes_not_dropped_or_overflowed() {
        let mut t = Template::new();
        for i in 0..(MAX_MESH_VERTICES / 4 + 1) {
            t.quad(
                [
                    vec3(i as f32, 0., 0.),
                    vec3(i as f32 + 1., 0., 0.),
                    vec3(i as f32 + 1., 1., 0.),
                    vec3(i as f32, 1., 0.),
                ],
                Vec3::Z,
                [1.; 3],
                0.,
            );
        }
        let mut b = Batch::new();
        b.add(&t, Mat4::IDENTITY, Tint::NONE);
        assert_eq!(b.vertex_count(), t.verts.len(), "every vertex is drawn");
        let drawn: Vec<_> = b.meshes.iter().filter(|m| !m.vertices.is_empty()).collect();
        assert!(drawn.len() >= 2, "split over several meshes");
        assert!(drawn
            .iter()
            .all(|m| m.vertices.len() <= MAX_MESH_VERTICES && m.indices.len() <= MAX_MESH_INDICES));
    }

    #[test]
    fn transformed_templates_move_points_and_rotate_normals() {
        let mut t = Template::new();
        t.box_(Vec3::ZERO, Vec3::ONE, [1.; 3], 0.);
        let m = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Mat4::from_translation(vec3(5., 0., 0.));
        let u = t.transformed(m);
        assert!(
            u.verts.iter().all(|v| v.p.x.abs() <= 1.01),
            "the translation was rotated onto z"
        );
        assert!(u.verts.iter().all(|v| (v.n.length() - 1.).abs() < 1e-4));
    }

    #[test]
    fn billboards_and_beams_pack_two_triangles_and_face_the_eye() {
        let mut b = Batch::new();
        b.billboard(Vec3::ZERO, Vec3::X, Vec3::Y, 2., 2., [1., 0., 0.], 0.5, 1.);
        b.beam(
            vec3(0., 0., 0.),
            vec3(0., 0., -4.),
            vec3(0., 5., -2.),
            0.2,
            [0., 1., 0.],
            1.,
            0.,
            1.,
        );
        assert_eq!((b.vertex_count(), b.meshes[0].indices.len()), (8, 12));
        let beam: Vec<_> = b.meshes[0].vertices[4..].iter().collect();
        assert!(
            beam.iter().all(|v| v.normal.y > 0.9),
            "the ribbon faces the eye above it"
        );
        assert_eq!(beam[0].color[3], 255);
        assert_eq!(beam[2].color[3], 0);
    }
}
