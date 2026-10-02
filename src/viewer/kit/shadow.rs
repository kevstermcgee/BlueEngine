//! Shadows for kit games: one setting, two tiers, one helper.
//!
//! [`Shadows`] is what a game uses. It owns the player's [`ShadowQuality`] and, depending on it:
//!
//! * `Off`: nothing. Every call is a no-op.
//! * `Simple`: soft **contact blobs** under moving things ([`Shadows::blob`]), drawn with the depth-tested
//!   `decal` material. No extra render pass; deterministic; cheap.
//! * `Full`: one directional **shadow map** of the key light ([`ShadowMap`]) around the action focus, so
//!   buildings, trees, walls and vehicles shadow everything. One extra pass over the casters.
//!
//! The rest of this module is the machinery behind `Full`, documented because its constraints are not
//! obvious (ADR 0036 has the long version):
//!
//! * macroquad exposes no depth texture, so the map is an **RGBA8 colour target** and the `caster` material
//!   writes the light-space depth packed into three bytes ([`pack_depth`]); the world shader unpacks it.
//!   The target is `Nearest`-filtered (linear filtering would blend the bytes of a packed value) and has
//!   `sample_count: 0` (a resolve target makes miniquad blit the whole target after every draw call).
//! * There is no polygon offset, and no derivatives in GLSL 100, so acne is controlled by **normal-offset
//!   bias** (receiver position pushed along its normal by about a texel and a half) plus a small constant
//!   depth bias. The offset is a world-space distance, so it scales with the box size automatically.
//! * The light box is orthographic and **texel-snapped**: the box moves in whole texels, so a point's
//!   texel never changes as the focus drifts and shadow edges do not shimmer.
//! * An unset `ShadowMap` sampler reads macroquad's 1x1 white texture, which unpacks to "infinitely far":
//!   unlit-by-shadow is the safe default, and a zero strength skips the lookup altogether.
//! * Pure Rust mirrors of the GLSL ([`pack_depth`], [`unpack_depth`], [`fit_light_box`],
//!   [`shadow_factor`]) are unit-tested without a GL context.
//!
//! Frame order for `Full` (the helper does the bookkeeping; the game keeps its own scene code):
//!
//! ```ignore
//! shadows.begin_frame(&look, focus);                 // fit the light box, clear the blob batch
//! world.clear(); /* fill the dynamic batches as usual */
//! shadows.cast(|| { statics.draw(); world.draw(); }); // shadow pass: same meshes, caster material
//! set_camera(&view.camera(0.3, 700.));
//! materials.set_scene(&look, view.eye, time, pulse);
//! shadows.apply(&materials);                         // bind the map to the world material
//! materials.draw_static(&scene);                     // static world
//! shadows.draw_decals(&materials);                   // Simple: blobs, after statics, before actors
//! world.draw();                                      // dynamic actors
//! ```
use macroquad::{camera::Camera, prelude::*, texture::RenderPass};

use super::batch::Batch;
use super::look::{Look, Materials};
use crate::viewer::devkit::ShadowQuality;

/// Smallest and largest shadow map side, in texels.
pub const MIN_RESOLUTION: u32 = 256;
pub const MAX_RESOLUTION: u32 = 4096;
/// Default shadow map side. 2048 over an 80 m box is 4 cm per texel.
pub const DEFAULT_RESOLUTION: u32 = 2048;
/// Default half-width of the light box around the focus, in metres.
pub const DEFAULT_HALF_EXTENT: f32 = 40.;
/// Default depth of the light box along the light direction, in metres (half towards the light).
pub const DEFAULT_DEPTH: f32 = 160.;
/// Default shadow darkness: 1 removes all key light in shadow (ambient and point lights remain).
pub const DEFAULT_STRENGTH: f32 = 0.85;
/// Receiver offset along its normal, in texels.
pub const NORMAL_OFFSET_TEXELS: f32 = 1.5;
/// Constant depth bias, in texels (metres along the light, texel = metres per texel).
pub const DEPTH_BIAS_TEXELS: f32 = 1.0;

/// Largest depth the packing can represent: 1 - 2^-20, so `1.0` (the cleared "far") never collides.
const PACK_MAX: f32 = 0.999_999;

const CASTER_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
varying highp float vdepth;
void main() {
    vec4 clip = Projection * Model * vec4(position, 1.0);
    gl_Position = clip;
    vdepth = clip.z / clip.w * 0.5 + 0.5;
}
"#;

const CASTER_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp float vdepth;
void main() {
    float d = clamp(vdepth, 0.0, 0.999999);
    vec3 enc = fract(d * vec3(1.0, 255.0, 65025.0));
    enc -= enc.yzz * vec3(1.0 / 255.0, 1.0 / 255.0, 0.0);
    gl_FragColor = vec4(enc, 1.0);
}
"#;

/// Pack a light-space depth in 0..1 into three bytes, exactly as the caster shader does (the hardware
/// rounds each channel to the nearest 1/255). Values outside the range are clamped.
pub fn pack_depth(depth: f32) -> [u8; 3] {
    let d = depth.clamp(0., PACK_MAX);
    let enc = [(d * 1.).fract(), (d * 255.).fract(), (d * 65025.).fract()];
    let enc = [enc[0] - enc[1] / 255., enc[1] - enc[2] / 255., enc[2]];
    enc.map(|c| (c * 255.).round().clamp(0., 255.) as u8)
}

/// Inverse of [`pack_depth`], exactly as the world shader does it. The cleared map (all 255) and the
/// 1x1 white default texture decode to a little over 1: farther than anything.
pub fn unpack_depth(rgb: [u8; 3]) -> f32 {
    let c = rgb.map(|b| f32::from(b) / 255.);
    c[0] + c[1] / 255. + c[2] / 65025.
}

/// A light-space box fitted around a focus point: the matrix, and what the shader needs to bias with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightBox {
    /// World to clip space (orthographic, OpenGL depth range).
    pub view_proj: Mat4,
    /// Metres per shadow texel.
    pub texel: f32,
    /// Box depth along the light, metres.
    pub depth: f32,
    /// Half-width of the box, metres.
    pub half_extent: f32,
    /// Shadow map side, texels.
    pub resolution: u32,
}

impl LightBox {
    /// World point to shadow-map texel coordinates `(x, y)` and light depth `z` in 0..1 (what the world
    /// shader computes; the texel coordinates have the map's origin at the lower-left corner).
    pub fn to_map(&self, p: Vec3) -> Vec3 {
        let clip = self.view_proj * p.extend(1.);
        let s = clip.truncate() / clip.w * 0.5 + 0.5;
        vec3(
            s.x * self.resolution as f32,
            s.y * self.resolution as f32,
            s.z,
        )
    }
}

/// Fit the orthographic box of the key light. `key_direction` points *towards* the light (as in
/// [`Look`]). The box is `2 * half_extent` wide and tall, `depth` long along the light with the focus in
/// the middle, and its position is snapped to whole texels in light space so a moving focus never makes
/// edges shimmer. Nonsense input (NaN, zero direction, tiny sizes) is repaired rather than propagated.
pub fn fit_light_box(
    key_direction: [f32; 3],
    focus: Vec3,
    half_extent: f32,
    depth: f32,
    resolution: u32,
) -> LightBox {
    let toward = Vec3::from(key_direction);
    let toward = if toward.is_finite() && toward.length_squared() > 1e-12 {
        toward.normalize()
    } else {
        Vec3::Y
    };
    let focus = if focus.is_finite() { focus } else { Vec3::ZERO };
    let half = if half_extent.is_finite() {
        half_extent.max(0.5)
    } else {
        DEFAULT_HALF_EXTENT
    };
    let depth = if depth.is_finite() {
        depth.max(2.)
    } else {
        DEFAULT_DEPTH
    };
    let resolution = resolution.clamp(MIN_RESOLUTION, MAX_RESOLUTION);
    let texel = 2. * half / resolution as f32;
    let up = if toward.y.abs() > 0.95 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    // Orientation only: the eye sits at the origin looking away from the light.
    let basis = Mat4::look_to_rh(Vec3::ZERO, -toward, up);
    let mut centre = basis.transform_point3(focus);
    centre.x = (centre.x / texel).round() * texel;
    centre.y = (centre.y / texel).round() * texel;
    // After this the (snapped) focus is at the box centre, `depth / 2` in front of the near plane.
    let view = Mat4::from_translation(vec3(-centre.x, -centre.y, -centre.z - depth * 0.5)) * basis;
    let proj = Mat4::orthographic_rh_gl(-half, half, -half, half, 0., depth);
    LightBox {
        view_proj: proj * view,
        texel,
        depth,
        half_extent: half,
        resolution,
    }
}

/// What the world shader decides for one surface point: 1 = fully lit by the key light, down to
/// `1 - strength` in shadow. `map` returns the unpacked depth stored at integer texel coordinates
/// (`None` outside the map); this mirrors the GLSL, including the normal offset, the edge fade and the
/// 3x3 filter, so tests can check bias, the far plane and the outside-the-map behaviour without a GPU.
pub fn shadow_factor(
    light: &LightBox,
    normal_offset: f32,
    depth_bias: f32,
    strength: f32,
    point: Vec3,
    normal: Vec3,
    map: impl Fn(i32, i32) -> Option<f32>,
) -> f32 {
    if strength <= 0. {
        return 1.;
    }
    let s = light.to_map(point + normal * normal_offset);
    let res = light.resolution as f32;
    let (u, v) = (s.x / res, s.y / res);
    let edge = ((u - 0.5).abs().max((v - 0.5).abs())) * 2.;
    let t = ((edge - 0.85) / 0.15).clamp(0., 1.);
    let fade = 1. - t * t * (3. - 2. * t);
    if s.z >= 1. || s.z <= 0. || fade <= 0. {
        return 1.;
    }
    let mut lit = 0.;
    for j in -1..=1 {
        for i in -1..=1 {
            // Nearest filtering: the texel containing the (offset) coordinate.
            let (x, y) = (
                (s.x + i as f32).floor() as i32,
                (s.y + j as f32).floor() as i32,
            );
            let stored = map(x, y).unwrap_or(2.);
            if s.z - depth_bias <= stored {
                lit += 1.;
            }
        }
    }
    let lit = lit / 9.;
    1. + (lit - 1.) * strength.min(1.) * fade
}

/// Camera of the shadow pass (an orthographic view along the key light that renders into the map).
/// Hand it to [`Materials::set_shadow`] for the lookup and to [`ShadowMap::pass`] (or `set_camera`) to draw.
pub struct ShadowCamera {
    light: LightBox,
    pass: RenderPass,
    normal_offset: f32,
    depth_bias: f32,
}

impl ShadowCamera {
    /// The fitted box.
    pub fn light(&self) -> &LightBox {
        &self.light
    }
    /// Receiver normal offset in metres and depth bias in 0..1 depth units, given in texels. The defaults
    /// ([`NORMAL_OFFSET_TEXELS`], [`DEPTH_BIAS_TEXELS`]) remove acne on flat and curved surfaces at the
    /// usual sun angles; raise them for a grazing sun, lower them if shadows detach from their casters.
    pub fn with_bias(mut self, normal_offset_texels: f32, depth_bias_texels: f32) -> Self {
        self.normal_offset = normal_offset_texels.max(0.) * self.light.texel;
        self.depth_bias = depth_bias_texels.max(0.) * self.light.texel / self.light.depth;
        self
    }
}

impl Camera for ShadowCamera {
    fn matrix(&self) -> Mat4 {
        self.light.view_proj
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<RenderPass> {
        Some(self.pass.clone())
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

/// The shadow map: an RGBA8 render target holding packed light-space depth, and the `caster` material
/// that writes it. Create after GL initialisation (like [`PlanarMirror`](super::PlanarMirror)).
pub struct ShadowMap {
    target: RenderTarget,
    resolution: u32,
    caster: Material,
}

impl ShadowMap {
    /// A square map of `resolution` texels per side (256 to 4096; 2048 is [`DEFAULT_RESOLUTION`]).
    pub fn new(resolution: u32) -> Result<Self, &'static str> {
        if !(MIN_RESOLUTION..=MAX_RESOLUTION).contains(&resolution) {
            return Err("shadow map resolution must be 256..=4096");
        }
        let caster = load_material(
            ShaderSource::Glsl {
                vertex: CASTER_VERTEX,
                fragment: CASTER_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: macroquad::miniquad::PipelineParams {
                    depth_test: macroquad::miniquad::Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|_| "the shadow caster shader failed to compile")?;
        // sample_count 0: no resolve texture, so miniquad does not blit the whole target after every draw.
        // depth: true gives hidden-surface removal inside the pass (the nearest surface wins the colour).
        let target = render_target_ex(
            resolution,
            resolution,
            RenderTargetParams {
                sample_count: 0,
                depth: true,
            },
        );
        // Packed bytes must reach the shader untouched: no interpolation between texels.
        target.texture.set_filter(FilterMode::Nearest);
        Ok(Self {
            target,
            resolution,
            caster,
        })
    }
    /// Texels per side.
    pub fn resolution(&self) -> u32 {
        self.resolution
    }
    /// The map as a texture (what the world material samples as `ShadowMap`).
    pub fn texture(&self) -> Texture2D {
        self.target.texture.clone()
    }
    /// The material that writes packed depth; bound for you by [`ShadowMap::pass`].
    pub fn caster(&self) -> &Material {
        &self.caster
    }
    /// Fit the key light's box around `focus` (usually the player or the camera target): `half_extent`
    /// metres each side, `depth` metres along the light. See [`fit_light_box`].
    pub fn camera(&self, look: &Look, focus: Vec3, half_extent: f32, depth: f32) -> ShadowCamera {
        let light = fit_light_box(
            look.key_direction,
            focus,
            half_extent,
            depth,
            self.resolution,
        );
        ShadowCamera {
            light,
            pass: self.target.render_pass.clone(),
            normal_offset: NORMAL_OFFSET_TEXELS * light.texel,
            depth_bias: DEPTH_BIAS_TEXELS * light.texel / light.depth,
        }
    }
    /// Run the shadow pass: clear the map to "far", bind the caster material and the light camera, call
    /// `draw` (draw every caster mesh now: `Batch::draw`, `draw_mesh`; do not change material), then return
    /// to the default camera and material. Call it before the frame's `set_camera` for the main view,
    /// and after filling the batches (the pass only reads CPU geometry).
    pub fn pass(&self, camera: &ShadowCamera, draw: impl FnOnce()) {
        set_camera(camera);
        clear_background(WHITE);
        gl_use_material(&self.caster);
        draw();
        gl_use_default_material();
        set_default_camera();
    }
}

/// How a blob shrinks as its caster rises: gone at this many radii above the ground.
pub const BLOB_FADE_RADII: f32 = 4.;
/// Highest strength of a blob at ground contact.
pub const BLOB_STRENGTH: f32 = 0.65;
/// How far below the ground a caster may sink before its blob is dropped (metres): a wheel in a rut keeps
/// its blob, a caster that fell through the floor does not leave a stain on top of it.
const BLOB_SINK: f32 = 0.5;
/// How high above the ground a blob sits, metres. The decal's own depth bias does the real work; this only
/// keeps it clear of float noise on a slightly uneven ground.
const BLOB_LIFT: f32 = 0.004;

/// The blob for something `height` metres above the ground: its radius and strength, or `None` when it
/// should not be drawn (non-finite, sunk through the ground, or too high to cast a contact shadow).
/// Strength falls off with the square of the height and the blob widens slightly: a soft contact shadow
/// for a hovering or jumping thing.
pub fn blob_for_height(radius: f32, height: f32, strength: f32) -> Option<(f32, f32)> {
    if !(radius.is_finite() && radius > 0. && height.is_finite() && strength.is_finite()) {
        return None;
    }
    if height < -BLOB_SINK {
        return None;
    }
    let t = (height.max(0.) / (radius * BLOB_FADE_RADII)).clamp(0., 1.);
    if t >= 1. {
        return None;
    }
    Some((
        radius * (1. + 0.3 * t),
        strength.clamp(0., 1.) * (1. - t) * (1. - t),
    ))
}

/// The game-facing shadow helper: owns the player's [`ShadowQuality`], the map (only when `Full`) and the
/// blob batch (only when `Simple`). Everything is a no-op at `Off`, and a game that never calls it renders
/// exactly as before. See the module documentation for the frame order.
///
/// ```ignore
/// let mut shadows = kit::Shadows::new(settings.shadow_quality);   // once, after the window exists
/// shadows.set_ground(|x, z| terrain_height(x, z));                // optional: default is flat at y = 0
/// // per frame:
/// shadows.begin_frame(&look, player_pos);
/// for kart in &karts { shadows.blob(kart.pos, 1.4); }             // Simple only
/// shadows.cast(|| { statics.draw(); actors.draw(); });            // Full only
/// /* set_camera(main); materials.set_scene(..); */
/// shadows.apply(&materials);
/// /* draw statics */ shadows.draw_decals(&materials); /* draw actors */
/// ```
pub struct Shadows {
    quality: ShadowQuality,
    resolution: u32,
    half_extent: f32,
    depth: f32,
    strength: f32,
    map: Option<ShadowMap>,
    camera: Option<ShadowCamera>,
    blobs: Batch,
    ground: Box<dyn Fn(f32, f32) -> f32>,
}

impl Shadows {
    /// A helper at `quality`. Call after the window (GL context) exists: `Full` allocates the map. If the
    /// map cannot be created the helper falls back to `Simple` (see [`Shadows::quality`]).
    pub fn new(quality: ShadowQuality) -> Self {
        let mut shadows = Self {
            quality: ShadowQuality::Off,
            resolution: DEFAULT_RESOLUTION,
            half_extent: DEFAULT_HALF_EXTENT,
            depth: DEFAULT_DEPTH,
            strength: DEFAULT_STRENGTH,
            map: None,
            camera: None,
            blobs: Batch::new(),
            ground: Box::new(|_, _| 0.),
        };
        shadows.set_quality(quality);
        shadows
    }
    /// Shadow map side in texels (256 to 4096, default 2048). Takes effect before the first `Full` frame.
    pub fn with_resolution(mut self, resolution: u32) -> Self {
        let resolution = resolution.clamp(MIN_RESOLUTION, MAX_RESOLUTION);
        if resolution != self.resolution {
            self.resolution = resolution;
            self.map = None;
            let quality = std::mem::replace(&mut self.quality, ShadowQuality::Off);
            self.set_quality(quality);
        }
        self
    }
    /// Size of the light box in metres: `half_extent` each side of the focus (default 40) and `depth` along
    /// the light (default 160). Smaller means crisper shadows; it must still reach every caster that can
    /// shadow the focus area (tall buildings towards the sun) and the focus's surroundings.
    pub fn with_range(mut self, half_extent: f32, depth: f32) -> Self {
        self.half_extent = half_extent;
        self.depth = depth;
        self
    }
    /// Shadow darkness 0-1 (default 0.85). 1 removes the whole key light in shadow; ambient stays.
    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0., 1.);
        self
    }
    /// Where the ground is, for blobs: `|x, z| height`. The default is flat at 0.
    pub fn set_ground(&mut self, ground: impl Fn(f32, f32) -> f32 + 'static) {
        self.ground = Box::new(ground);
    }
    /// The tier actually in effect (`Full` requested but unavailable reads as `Simple`).
    pub fn quality(&self) -> ShadowQuality {
        self.quality
    }
    /// Change the tier (the Settings screen does this). Allocates the map the first time `Full` is chosen.
    pub fn set_quality(&mut self, quality: ShadowQuality) {
        self.quality = quality;
        if quality == ShadowQuality::Full {
            if self.map.is_none() {
                self.map = ShadowMap::new(self.resolution).ok();
            }
            if self.map.is_none() {
                self.quality = ShadowQuality::Simple;
            }
        }
        if self.quality != ShadowQuality::Full {
            self.camera = None;
        }
    }
    /// Start a frame: forget last frame's blobs and, at `Full`, fit the light box around `focus` (the human
    /// player, the camera target: whatever the box should follow).
    pub fn begin_frame(&mut self, look: &Look, focus: Vec3) {
        self.blobs.clear();
        self.camera = match (&self.map, self.quality) {
            (Some(map), ShadowQuality::Full) => {
                Some(map.camera(look, focus, self.half_extent, self.depth))
            }
            _ => None,
        };
    }
    /// True when [`Shadows::cast`] will run its closure this frame (so a game can skip building casters
    /// otherwise).
    pub fn casting(&self) -> bool {
        self.camera.is_some()
    }
    /// The shadow pass. At `Full`, calls `draw` with the caster material bound and the light camera active;
    /// draw every mesh that should cast (statics, actors; not fx, glass or a first-person viewmodel). At
    /// other tiers it does nothing and `draw` is not called. Fill your batches first; call before the
    /// frame's `set_camera` for the main view.
    pub fn cast(&self, draw: impl FnOnce()) {
        if let (Some(map), Some(camera)) = (&self.map, &self.camera) {
            map.pass(camera, draw);
        }
    }
    /// Bind this frame's map to the world material, or switch shadowing off for it. Call every frame after
    /// `Materials::set_scene` (the Off and Simple tiers need the call to clear a map left from `Full`).
    pub fn apply(&self, materials: &Materials) {
        match (&self.map, &self.camera) {
            (Some(map), Some(camera)) => materials.set_shadow(map, camera, self.strength),
            _ => materials.clear_shadow(),
        }
    }
    /// Simple tier: a contact blob under something at `pos` (its position on or above the ground: the feet,
    /// the bottom of a kart) with `radius` metres. The ground height comes from [`Shadows::set_ground`];
    /// the blob fades and widens as the thing rises and disappears under the ground. No-op unless the tier
    /// is `Simple` (at `Full` the real shadow does the job).
    pub fn blob(&mut self, pos: Vec3, radius: f32) {
        if self.quality != ShadowQuality::Simple || !pos.is_finite() {
            return;
        }
        let ground = (self.ground)(pos.x, pos.z);
        if !ground.is_finite() {
            return;
        }
        if let Some((radius, strength)) = blob_for_height(radius, pos.y - ground, BLOB_STRENGTH) {
            self.blobs
                .blob(vec3(pos.x, ground + BLOB_LIFT, pos.z), radius, strength);
        }
    }
    /// Draw this frame's blobs with the depth-tested `decal` material. Call after the static world and
    /// before the dynamic actors.
    pub fn draw_decals(&self, materials: &Materials) {
        if self.blobs.vertex_count() > 0 {
            gl_use_material(&materials.decal);
            self.blobs.draw();
        }
    }
}

impl Materials {
    /// Bind `map` and its `camera` to the world material for the main pass: the key light's term is darkened
    /// by up to `strength` (0..1) wherever the map says a caster is in the way. Call after
    /// [`Materials::set_scene`] and the shadow pass. Only the key light is shadowed; point lights are not.
    pub fn set_shadow(&self, map: &ShadowMap, camera: &ShadowCamera, strength: f32) {
        self.world.set_texture("ShadowMap", map.texture());
        self.world.set_uniform("LightVP", camera.light.view_proj);
        self.world.set_uniform(
            "Shadow",
            vec4(
                if strength.is_finite() {
                    strength.clamp(0., 1.)
                } else {
                    0.
                },
                1. / map.resolution as f32,
                camera.normal_offset,
                camera.depth_bias,
            ),
        );
    }
    /// Stop shadowing (strength 0: the shader skips the lookup). This is also the state before any
    /// [`Materials::set_shadow`], so games that never opt in render exactly as before.
    pub fn clear_shadow(&self) {
        self.world.set_uniform("Shadow", Vec4::ZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::super::batch::{blob_template, Template};
    use super::super::lint;
    use super::super::look::{DECAL_DEPTH_BIAS, WORLD_FRAGMENT};
    use super::*;

    fn texel_of(light: &LightBox, p: Vec3) -> Vec2 {
        light.to_map(p).truncate()
    }

    #[test]
    fn packed_depth_round_trips_within_a_few_millionths() {
        let mut worst = 0f32;
        for i in 0..=20_000u32 {
            let d = i as f32 / 20_000.;
            let back = unpack_depth(pack_depth(d));
            worst = worst.max((back - d.min(PACK_MAX)).abs());
        }
        // 24 bits over the box depth: well under a millimetre for any box up to a kilometre.
        assert!(worst < 2e-6, "worst round-trip error {worst}");
        // Finer than one texel of a 4 cm map even at the largest sensible box depth (200 m).
        assert!(worst * 200. < 0.04 / 10.);
        // The packing is monotonic: a nearer surface never decodes as farther.
        let mut previous = -1.;
        for i in 0..=5000u32 {
            let v = unpack_depth(pack_depth(i as f32 / 5000.));
            assert!(v >= previous, "packing went backwards at {i}");
            previous = v;
        }
    }

    #[test]
    fn cleared_map_and_unset_texture_decode_as_farther_than_everything() {
        assert!(unpack_depth([255, 255, 255]) > 1.);
        assert!(
            unpack_depth(pack_depth(5.)) <= 1.,
            "out of range input is clamped, not wrapped"
        );
        assert!(unpack_depth(pack_depth(f32::MAX)) < unpack_depth([255, 255, 255]));
        assert_eq!(pack_depth(-3.), [0, 0, 0]);
    }

    #[test]
    fn the_box_is_centred_on_the_focus_and_looks_along_the_light() {
        let look = Look::daylight();
        let focus = vec3(12.3, 1.7, -45.6);
        let light = fit_light_box(look.key_direction, focus, 30., 100., 2048);
        let c = light.to_map(focus);
        // Centre of the map (to within the snapping, at most half a texel), half way through the depth.
        assert!(
            (c.x - 1024.).abs() <= 1. && (c.y - 1024.).abs() <= 1.,
            "{c:?}"
        );
        assert!((c.z - 0.5).abs() < 1e-4, "{c:?}");
        // A point nearer the light has a smaller depth than one farther from it.
        let toward = Vec3::from(look.key_direction).normalize();
        assert!(light.to_map(focus + toward * 10.).z < c.z);
        assert!(light.to_map(focus - toward * 10.).z > c.z);
        // Beyond the box along the light: outside 0..1 (shader treats it as lit).
        assert!(light.to_map(focus + toward * 60.).z < 0.);
        assert!(light.to_map(focus - toward * 60.).z > 1.);
        assert!((light.texel - 60. / 2048.).abs() < 1e-7);
    }

    #[test]
    fn a_world_x_axis_shadow_points_away_from_the_light() {
        // A caster 3 m above the origin shadows the ground at the light's horizontal opposite.
        let toward = Vec3::from([-0.4, 0.8, -0.35]).normalize();
        let caster = vec3(0., 3., 0.);
        let on_ground = caster - toward * (3. / toward.y);
        assert!(on_ground.x > 0. && on_ground.z > 0.);
        let light = fit_light_box([-0.4, 0.8, -0.35], Vec3::ZERO, 20., 80., 2048);
        let (a, b) = (light.to_map(caster), light.to_map(on_ground));
        // Same line of sight along the light: identical map texel, different depth, ground farther.
        assert!((a.truncate() - b.truncate()).length() < 1e-2, "{a:?} {b:?}");
        assert!(b.z > a.z);
    }

    #[test]
    fn snapping_moves_the_box_in_whole_texels_so_edges_do_not_shimmer() {
        let look = Look::dusk();
        let probe = vec3(3.1, 0.4, -7.7);
        let base = fit_light_box(look.key_direction, vec3(5., 0., 5.), 25., 120., 1024);
        let at = texel_of(&base, probe);
        let mut changed = 0;
        for i in 0..200 {
            let t = i as f32 * 0.0137;
            let focus = vec3(5. + t, 0., 5. - t * 0.6);
            let moved = fit_light_box(look.key_direction, focus, 25., 120., 1024);
            let now = texel_of(&moved, probe);
            let shift = now - at;
            // The probe's position inside its texel must not change: the shift is a whole number of texels.
            for axis in [shift.x, shift.y] {
                assert!(
                    (axis - axis.round()).abs() < 5e-3,
                    "sub-texel drift {axis} at step {i}"
                );
            }
            if moved.view_proj != base.view_proj {
                changed += 1;
            }
        }
        assert!(changed > 0, "the box must follow the focus eventually");
        // Moving by less than a texel inside the same cell leaves the matrix bit-identical.
        let a = fit_light_box(look.key_direction, vec3(5., 0., 5.), 25., 120., 1024);
        let tiny = a.texel * 0.01;
        let b = fit_light_box(look.key_direction, vec3(5. + tiny, 0., 5.), 25., 120., 1024);
        assert!(
            (a.view_proj - b.view_proj).abs_diff_eq(Mat4::ZERO, 1e-3) || a.view_proj == b.view_proj
        );
    }

    #[test]
    fn nonsense_input_is_repaired_not_propagated() {
        for light in [
            fit_light_box([0.; 3], Vec3::ZERO, 10., 50., 1024),
            fit_light_box(
                [f32::NAN, 1., 0.],
                vec3(f32::NAN, 0., 0.),
                f32::NAN,
                f32::INFINITY,
                7,
            ),
            fit_light_box([0., 1., 0.], Vec3::ZERO, -5., -5., 1_000_000),
            fit_light_box([0., -1., 0.], Vec3::ZERO, 10., 50., 1024),
        ] {
            assert!(light.view_proj.is_finite());
            assert!(light.texel > 0. && light.depth >= 2. && light.half_extent >= 0.5);
            assert!((MIN_RESOLUTION..=MAX_RESOLUTION).contains(&light.resolution));
        }
    }

    /// A tiny CPU shadow map: rasterise a horizontal slab of casters as the packed map would hold them.
    fn slab_map(light: &LightBox, y: f32, half: f32) -> impl Fn(i32, i32) -> Option<f32> + '_ {
        move |x, z| {
            if x < 0 || z < 0 || x >= light.resolution as i32 || z >= light.resolution as i32 {
                return None;
            }
            // Which world point on the slab's plane does this texel see? Walk the texel's ray.
            let uv = vec2(
                (x as f32 + 0.5) / light.resolution as f32 * 2. - 1.,
                (z as f32 + 0.5) / light.resolution as f32 * 2. - 1.,
            );
            let inv = light.view_proj.inverse();
            let near = inv.project_point3(uv.extend(-1.));
            let far = inv.project_point3(uv.extend(1.));
            let dir = far - near;
            let t = (y - near.y) / dir.y;
            let hit = near + dir * t;
            if hit.x.abs() <= half && hit.z.abs() <= half {
                // The box is orthographic, so t (0 at the near plane, 1 at the far) is the stored depth.
                Some(unpack_depth(pack_depth(t)))
            } else {
                Some(2.)
            }
        }
    }

    #[test]
    fn shadow_factor_shadows_below_a_caster_and_lights_elsewhere() {
        let toward = [0., 1., 0.];
        let light = fit_light_box(toward, Vec3::ZERO, 20., 80., 1024);
        let map = slab_map(&light, 5., 3.);
        let (off, bias, strength) = (0.05, 0.001, 0.8);
        let under = shadow_factor(&light, off, bias, strength, vec3(0., 0., 0.), Vec3::Y, &map);
        assert!((under - 0.2).abs() < 1e-3, "under the slab: {under}");
        let beside = shadow_factor(
            &light,
            off,
            bias,
            strength,
            vec3(10., 0., 0.),
            Vec3::Y,
            &map,
        );
        assert_eq!(beside, 1., "beside the slab is lit");
        // The slab's own top surface is not in its own shadow: bias sign. A receiver at the caster's depth
        // (or a hair behind it, within the bias) stays lit; one clearly behind is shadowed.
        let top = shadow_factor(&light, 0., bias, strength, vec3(0., 5., 0.), Vec3::Y, &map);
        assert_eq!(top, 1., "a surface at the caster's depth is lit (no acne)");
        let behind = shadow_factor(&light, 0., bias, strength, vec3(0., 4., 0.), Vec3::Y, &map);
        assert!(
            behind < 0.3,
            "clearly behind the caster is shadowed: {behind}"
        );
        // Strength 0 disables everything.
        assert_eq!(
            shadow_factor(&light, off, bias, 0., Vec3::ZERO, Vec3::Y, &map),
            1.
        );
    }

    #[test]
    fn outside_the_box_or_beyond_the_far_plane_is_lit() {
        let light = fit_light_box([0., 1., 0.], Vec3::ZERO, 20., 80., 1024);
        let everything_shadowed = |_: i32, _: i32| Some(0.0);
        let at = |p: Vec3| shadow_factor(&light, 0., 0., 1., p, Vec3::Y, everything_shadowed);
        assert!(
            at(vec3(0., 0., 0.)) < 0.01,
            "inside the box the stub map shadows everything"
        );
        assert_eq!(at(vec3(500., 0., 0.)), 1., "outside the box sideways");
        assert_eq!(
            at(vec3(0., -100., 0.)),
            1.,
            "beyond the far plane (behind the focus, away from the light)"
        );
        assert_eq!(
            at(vec3(0., 100., 0.)),
            1.,
            "nearer the light than the near plane"
        );
        // The edge fades out rather than popping.
        let edge = at(vec3(19.5, 0., 0.));
        assert!(edge > 0.1 && edge < 1., "inside the fade band: {edge}");
    }

    #[test]
    fn normal_offset_keeps_a_sloped_receiver_out_of_its_own_shadow() {
        // A 45 degree slope lit at grazing 30 degrees: the map texel stores the surface depth at the texel
        // centre, the receiver sits up to half a texel off it, so without bias half the surface would shadow
        // itself. The shipped bias (1.5 texels normal offset + 1 texel depth) must clear that.
        let toward = Vec3::new(0.5, 0.866, 0.).normalize();
        let normal = Vec3::new(-1., 1., 0.).normalize();
        let light = fit_light_box(toward.to_array(), Vec3::ZERO, 20., 80., 1024);
        let plane = move |p: Vec3| normal.dot(p); // surface: normal . p = 0
        let slope_map = |x: i32, y: i32| {
            // Depth of the sloped plane seen through texel (x, y).
            let uv = vec2(
                (x as f32 + 0.5) / 1024. * 2. - 1.,
                (y as f32 + 0.5) / 1024. * 2. - 1.,
            );
            let inv = light.view_proj.inverse();
            let near = inv.project_point3(uv.extend(-1.));
            let far = inv.project_point3(uv.extend(1.));
            let (a, b) = (plane(near), plane(far));
            let t = a / (a - b);
            Some(unpack_depth(pack_depth(t)))
        };
        let (off, bias) = (
            NORMAL_OFFSET_TEXELS * light.texel,
            DEPTH_BIAS_TEXELS * light.texel / light.depth,
        );
        let mut shadowed = 0;
        let mut total = 0;
        for i in 0..400 {
            let along = -8. + i as f32 * 0.04;
            let p = vec3(along, along, 0.7 * (i % 7) as f32 - 2.);
            let f = shadow_factor(&light, off, bias, 1., p, normal, slope_map);
            total += 1;
            if f < 0.99 {
                shadowed += 1;
            }
        }
        assert_eq!(
            shadowed, 0,
            "{shadowed} of {total} receivers on a lit slope shadowed themselves"
        );
        // And the same slope with no bias at all does acne, proving the test can fail.
        let mut acne = 0;
        for i in 0..400 {
            let along = -8. + i as f32 * 0.04;
            let p = vec3(along, along, 0.7 * (i % 7) as f32 - 2.);
            if shadow_factor(&light, 0., 0., 1., p, normal, slope_map) < 0.99 {
                acne += 1;
            }
        }
        assert!(
            acne > 0,
            "without bias this slope must self-shadow, or the test checks nothing"
        );
    }

    #[test]
    fn the_shader_reads_what_the_caster_writes() {
        // The two GLSL snippets and the Rust mirror must use the same constants.
        for needle in [
            "vec3(1.0, 255.0, 65025.0)",
            "enc.yzz * vec3(1.0 / 255.0, 1.0 / 255.0, 0.0)",
        ] {
            assert!(CASTER_FRAGMENT.contains(needle), "{needle}");
        }
        assert!(WORLD_FRAGMENT.contains("vec3(1.0, 1.0 / 255.0, 1.0 / 65025.0)"));
        assert!(
            WORLD_FRAGMENT.contains("lit / 9.0")
                && WORLD_FRAGMENT.contains("smoothstep(0.85, 1.0, edge)")
        );
        assert!(CASTER_FRAGMENT.contains("0.999999") && PACK_MAX == 0.999_999);
    }

    #[test]
    fn resolution_bounds_are_enforced_without_a_gl_context() {
        // ShadowMap::new needs GL for the allowed range; the bounds check comes first, so bad sizes fail
        // cleanly even here.
        assert!(ShadowMap::new(0).is_err());
        assert!(ShadowMap::new(100).is_err());
        assert!(ShadowMap::new(100_000).is_err());
    }

    // ---- Simple tier: blobs and the decal depth bias ----

    #[test]
    fn the_unit_blob_is_lint_clean_round_soft_and_flat() {
        let blob = blob_template();
        lint::assert_clean(blob, "blob");
        assert!(blob.verts.iter().all(|v| v.p.y == 0. && v.n == Vec3::Y));
        // Opaque at the middle, nothing at the rim, and opacity never rises with radius.
        let mut by_radius: Vec<(f32, f32)> = blob
            .verts
            .iter()
            .map(|v| (v.p.xz().length(), v.a))
            .collect();
        by_radius.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(
            (by_radius[0].1 - 1.).abs() < 1e-6,
            "the middle is fully dark"
        );
        let last = by_radius.last().unwrap();
        assert!(
            (last.0 - 1.).abs() < 1e-5 && last.1 < 1e-6,
            "the rim fades to nothing: {last:?}"
        );
        assert!(
            by_radius.windows(2).all(|w| w[1].1 <= w[0].1 + 1e-6),
            "opacity falls with radius"
        );
        // Vertex count stays tiny: eight karts are a few hundred vertices.
        assert!(blob.verts.len() < 250, "{}", blob.verts.len());
    }

    #[test]
    fn batch_blob_places_scales_tints_and_refuses_nonsense() {
        let mut batch = Batch::new();
        batch.blob(vec3(10., 0.5, -4.), 2., 0.5);
        let n = batch.vertex_count();
        assert_eq!(n, blob_template().verts.len());
        let mesh = &batch.meshes[0];
        let centre = mesh.vertices.iter().find(|v| v.color[3] > 0).unwrap();
        assert!((centre.position - vec3(10., 0.5, -4.)).length() < 1e-5);
        let max_alpha = mesh.vertices.iter().map(|v| v.color[3]).max().unwrap();
        assert_eq!(max_alpha, 128, "strength 0.5 of full opacity");
        let widest = mesh
            .vertices
            .iter()
            .map(|v| (v.position - vec3(10., 0.5, -4.)).length())
            .fold(0., f32::max);
        assert!((widest - 2.).abs() < 1e-4, "radius 2 m: {widest}");
        for (c, r, s) in [
            (Vec3::NAN, 1., 1.),
            (Vec3::ZERO, 0., 1.),
            (Vec3::ZERO, -1., 1.),
            (Vec3::ZERO, f32::NAN, 1.),
            (Vec3::ZERO, 1., f32::NAN),
            (Vec3::ZERO, 1., 0.),
        ] {
            batch.blob(c, r, s);
        }
        assert_eq!(batch.vertex_count(), n, "nonsense adds nothing");
        let before = batch.vertex_count();
        batch.blob(Vec3::ZERO, 1., 7.);
        let added = &batch.meshes[0].vertices[before..];
        assert_eq!(
            added.iter().map(|v| v.color[3]).max(),
            Some(255),
            "strength clamps to 1"
        );
    }

    #[test]
    fn a_blob_on_a_road_is_lint_clean_with_its_lift_and_the_bias_does_the_rest() {
        let mut road = Template::new();
        road.quad_facing(
            [
                vec3(-5., 0., -50.),
                vec3(5., 0., -50.),
                vec3(5., 0., 50.),
                vec3(-5., 0., 50.),
            ],
            Vec3::Y,
            [0.3; 3],
            0.,
        );
        let mut scene = Batch::new();
        scene.blob(vec3(0., BLOB_LIFT, 0.), 1.5, 1.);
        let mut all = road.clone();
        all.append(
            &blob_template().transformed(Mat4::from_scale_rotation_translation(
                Vec3::splat(1.5),
                Quat::IDENTITY,
                vec3(0., BLOB_LIFT, 0.),
            )),
        );
        lint::assert_clean(&all, "road + blob");
        assert!(BLOB_LIFT > lint::LintConfig::default().plane_epsilon);
    }

    /// Depth-buffer value (24 bit) of a point `d` metres in front of a perspective camera.
    fn quantised_depth(near: f32, far: f32, d: f64, ndc_shift: f64) -> i64 {
        let (n, f) = (f64::from(near), f64::from(far));
        let ndc = (f + n) / (f - n) - 2. * f * n / ((f - n) * d) - ndc_shift;
        ((ndc * 0.5 + 0.5) * 16_777_215.).floor() as i64
    }

    #[test]
    fn a_decal_beats_a_coplanar_road_at_every_distance_with_the_real_planes() {
        let (near, far) = (0.3_f32, 700_f32);
        let shift = f64::from(DECAL_DEPTH_BIAS);
        let mut d = 0.35_f64;
        while d < 699. {
            let road = quantised_depth(near, far, d, 0.);
            let decal = quantised_depth(near, far, d, shift);
            // At least 7 buffer steps in front (8 less one for flooring), so interpolation noise cannot flip it.
            assert!(
                road - decal >= 7,
                "decal not in front at {d} m: {road} vs {decal}"
            );
            d *= 1.07;
        }
        // The bias expressed in metres is a fixed multiple of the buffer's resolution at that distance, and
        // far smaller than anything a viewer could read as "in front".
        for distance in [5_f32, 50., 300., 699.] {
            let margin = DECAL_DEPTH_BIAS * (far - near) * distance * distance / (2. * far * near);
            let resolution = lint::depth_resolution(near, distance);
            assert!(
                (6. ..=10.).contains(&(margin / resolution)),
                "{margin} m vs one buffer step {resolution} m at {distance} m"
            );
        }
        // Close up a blob still hides behind a wall only a few centimetres in front of it.
        let margin_at_20 = DECAL_DEPTH_BIAS * (far - near) * 400. / (2. * far * near);
        assert!(margin_at_20 < 0.005, "{margin_at_20} m at 20 m");
    }

    #[test]
    fn a_decal_behind_a_wall_stays_hidden_and_one_under_an_actor_is_covered() {
        // One pixel column: depth test LessOrEqual with depth writes, as the decal material does.
        let (near, far) = (0.3_f32, 700_f32);
        let shift = f64::from(DECAL_DEPTH_BIAS);
        for distance in [3_f64, 12., 40., 90.] {
            let ground = quantised_depth(near, far, distance, 0.);
            let decal = quantised_depth(near, far, distance, shift);
            let wall = quantised_depth(near, far, distance - 0.15, 0.); // a wall 15 cm in front of the blob
            let actor = quantised_depth(near, far, distance - 0.4, 0.); // a body hovering 40 cm nearer
                                                                        // Order: static world, decal, actor. A wall that is part of the static world is drawn first.
            let mut depth = i64::MAX;
            let mut shown = "sky";
            for (name, z) in [("wall", wall), ("ground", ground), ("decal", decal)] {
                if z <= depth {
                    depth = z;
                    shown = name;
                }
            }
            assert_eq!(shown, "wall", "the wall hides the blob at {distance} m");
            // Without the wall: the decal wins over the ground, then the actor over the decal.
            let mut depth = i64::MAX;
            let mut shown = "sky";
            for (name, z) in [("ground", ground), ("decal", decal), ("actor", actor)] {
                if z <= depth {
                    depth = z;
                    shown = name;
                }
            }
            assert_eq!(
                shown, "actor",
                "the actor covers its own blob at {distance} m"
            );
        }
    }

    #[test]
    fn the_decal_vertex_shader_carries_the_bias_and_the_world_one_does_not() {
        let decal = super::super::look::decal_vertex_source();
        assert!(decal.contains("gl_Position.z -= "), "{decal}");
        assert!(decal.contains(&format!("{:e}", DECAL_DEPTH_BIAS)));
        assert!(
            decal.contains("* gl_Position.w;"),
            "the bias is in clip space, scaled by w"
        );
    }

    #[test]
    fn blobs_follow_the_ground_fade_with_height_and_vanish_when_sunk_or_too_high() {
        assert_eq!(blob_for_height(1., 0., 0.5), Some((1., 0.5)));
        let (r1, s1) = blob_for_height(1., 1., 0.5).unwrap();
        let (r2, s2) = blob_for_height(1., 2., 0.5).unwrap();
        assert!(r1 > 1. && r2 > r1, "wider as it rises");
        assert!(s1 < 0.5 && s2 < s1, "fainter as it rises");
        assert_eq!(blob_for_height(1., BLOB_FADE_RADII, 0.5), None);
        assert_eq!(blob_for_height(1., 100., 0.5), None);
        assert!(
            blob_for_height(1., -0.3, 0.5).is_some(),
            "a wheel in a rut keeps its blob"
        );
        assert_eq!(
            blob_for_height(1., -2., 0.5),
            None,
            "fallen through the floor"
        );
        for (r, h, s) in [
            (f32::NAN, 0., 1.),
            (0., 0., 1.),
            (1., f32::NAN, 1.),
            (1., 0., f32::INFINITY),
        ] {
            assert_eq!(blob_for_height(r, h, s), None);
        }
    }

    #[test]
    fn shadows_off_does_nothing_and_simple_draws_blobs_on_the_ground_function() {
        let look = Look::daylight();
        let mut shadows = Shadows::new(ShadowQuality::Off);
        shadows.begin_frame(&look, Vec3::ZERO);
        shadows.blob(vec3(1., 0., 1.), 1.);
        assert_eq!(shadows.blobs.vertex_count(), 0, "Off adds no blobs");
        assert!(!shadows.casting());
        let mut ran = false;
        shadows.cast(|| ran = true);
        assert!(!ran, "Off never runs the shadow pass");

        shadows.set_quality(ShadowQuality::Simple);
        shadows.set_ground(|x, _| 0.1 * x);
        shadows.begin_frame(&look, Vec3::ZERO);
        shadows.blob(vec3(10., 1.2, 5.), 1.);
        assert!(shadows.blobs.vertex_count() > 0);
        let centre = shadows.blobs.meshes[0]
            .vertices
            .iter()
            .max_by_key(|v| v.color[3])
            .unwrap();
        assert!(
            (centre.position.y - (1. + BLOB_LIFT)).abs() < 1e-5,
            "{:?}",
            centre.position
        );
        assert!(!shadows.casting());
        shadows.cast(|| ran = true);
        assert!(!ran, "Simple never runs the shadow pass either");
        // A new frame forgets the old blobs.
        shadows.begin_frame(&look, Vec3::ZERO);
        assert_eq!(shadows.blobs.vertex_count(), 0);
        // Hovering 10 m up: too high for a contact shadow.
        shadows.blob(vec3(0., 10., 0.), 1.);
        assert_eq!(shadows.blobs.vertex_count(), 0);
    }
}
