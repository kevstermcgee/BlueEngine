//! Scene lighting and the materials that draw dynamic geometry.
//!
//! The stock material (`mesh::material`) is tuned for baked house interiors. A game whose look is
//! neon, a desert or a night sky wants its own light, fog and glow, and the smallest way to get them is
//! four ready-made materials driven by one plain-data [`Look`]:
//!
//! | Material | Use |
//! |---|---|
//! | `world` | opaque, lit by a hemisphere ambient, one key light and a rim, glow, tone-mapped, fogged |
//! | `fx_alpha` | translucent surfaces that do not write depth (shadows blobs, glass, halos); **never depth-tested either**, see below |
//! | `fx_add` | additive glow: sparks, beams, shockwaves; likewise never depth-tested |
//! | `sky` | unlit vertex-coloured dome, drawn first without depth |
//!
//! **`fx_alpha` and `fx_add` are not depth-tested.** Their pipelines ask for `depth_test: LessOrEqual` with
//! `depth_write: false`, but miniquad 0.4.8 (`src/graphics/gl.rs`, `apply_pipeline`, around line 1303) only
//! enables `GL_DEPTH_TEST` when `depth_write` is true and otherwise calls `glDisable(GL_DEPTH_TEST)`, so the
//! requested comparison is silently ignored. Everything drawn with these two materials lands on top of the
//! opaque world, whatever stands in front of it: a glow, a shadow blob or a pane of glass behind a wall is
//! visible through the wall. Until the kit has a depth-tested translucent path, keep such geometry from
//! being occluded (fade it with distance, cull it yourself, or draw it with `world` and a baked alpha
//! cut-out instead).
//!
//! Geometry comes from [`Template`](super::Template)/[`Batch`](super::Batch) (vertex `uv.x` is glow,
//! `normal.xyz` the normal) or from `mesh::bake_with` for a static world.
use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams, UniformDesc,
    UniformType,
};
use macroquad::prelude::*;

use super::batch::Rgb;

/// A finite-radius, unshadowed local light. Colors are linear RGB; intensity is a multiplier.
/// The bounded kit supports four lights per pass. Invalid values are refused at construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    position: Vec3,
    radius: f32,
    color: Vec3,
    intensity: f32,
}
impl PointLight {
    pub fn new(
        position: Vec3,
        radius: f32,
        color: Rgb,
        intensity: f32,
    ) -> Result<Self, &'static str> {
        let color = Vec3::from(color);
        if !position.is_finite()
            || !radius.is_finite()
            || radius <= 0.
            || !color.is_finite()
            || color.min_element() < 0.
            || !intensity.is_finite()
            || intensity < 0.
        {
            return Err(
                "point light needs finite values, positive radius and nonnegative color/intensity",
            );
        }
        Ok(Self {
            position,
            radius,
            color,
            intensity,
        })
    }
    /// Diffuse contribution before surface color/tone mapping; mirrors the shader's radial falloff.
    pub fn diffuse_at(&self, point: Vec3, normal: Vec3) -> Vec3 {
        if !point.is_finite() || !normal.is_finite() {
            return Vec3::ZERO;
        }
        let delta = self.position - point;
        let d = delta.length();
        let diffuse = normal
            .normalize_or_zero()
            .dot(delta / d.max(0.0001))
            .max(0.);
        let falloff = (1. - d / self.radius).clamp(0., 1.);
        self.color * (self.intensity * diffuse * falloff * falloff)
    }
}
/// Maximum local lights in one kit render pass.
pub const MAX_POINT_LIGHTS: usize = 4;

/// Everything about the scene's light that the materials read. Plain numbers, so a game can animate
/// them (fade the fog at a boss, pulse the glow on the beat).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Ambient light from above (surfaces facing +Y).
    pub ambient_sky: Rgb,
    /// Ambient light from below (surfaces facing -Y).
    pub ambient_ground: Rgb,
    /// Direction *towards* the key light (need not be normalised).
    pub key_direction: [f32; 3],
    /// Key light colour and intensity.
    pub key_color: Rgb,
    /// Rim light colour, seen on surfaces at grazing angles.
    pub rim_color: Rgb,
    /// Rim light strength (0 disables it).
    pub rim_strength: f32,
    /// Distance fog colour; also a good `clear_background` colour.
    pub fog_color: Rgb,
    /// Fog density per metre (0.008 is a light haze at 100 m).
    pub fog_density: f32,
    /// Final brightness multiplier (1 = as is; fade to black with 0).
    pub exposure: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self::daylight()
    }
}

impl Look {
    /// Bright day: blue sky ambient, warm sun, light haze.
    pub fn daylight() -> Self {
        Self {
            ambient_sky: [0.42, 0.50, 0.62],
            ambient_ground: [0.24, 0.22, 0.20],
            key_direction: [-0.4, 0.8, -0.35],
            key_color: [1.0, 0.93, 0.80],
            rim_color: [0.55, 0.65, 0.85],
            rim_strength: 0.15,
            fog_color: [0.62, 0.74, 0.88],
            fog_density: 0.006,
            exposure: 1.,
        }
    }
    /// Low sun: warm, long shadows' worth of contrast, orange haze.
    pub fn dusk() -> Self {
        Self {
            ambient_sky: [0.32, 0.26, 0.40],
            ambient_ground: [0.20, 0.14, 0.12],
            key_direction: [0.7, 0.28, -0.5],
            key_color: [1.0, 0.62, 0.34],
            rim_color: [0.9, 0.45, 0.35],
            rim_strength: 0.3,
            fog_color: [0.55, 0.32, 0.30],
            fog_density: 0.010,
            exposure: 1.,
        }
    }
    /// Night: dim cool ambient, moon key light, strong rim, dense dark fog (neon looks good here).
    pub fn night() -> Self {
        Self {
            ambient_sky: [0.14, 0.12, 0.24],
            ambient_ground: [0.08, 0.06, 0.12],
            key_direction: [-0.35, 0.85, -0.40],
            key_color: [0.55, 0.52, 0.85],
            rim_color: [0.30, 0.24, 0.58],
            rim_strength: 0.35,
            fog_color: [0.09, 0.05, 0.16],
            fog_density: 0.012,
            exposure: 1.,
        }
    }
    /// The fog colour as a macroquad colour: pass it to `clear_background`.
    pub fn clear_color(&self) -> Color {
        Color::new(self.fog_color[0], self.fog_color[1], self.fog_color[2], 1.)
    }
}

const WORLD_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
varying lowp vec4 vcolor;
varying mediump vec3 vnormal;
varying highp vec3 vpos;
varying lowp float vemis;
void main() {
    vec4 wp = Model * vec4(position, 1.0);
    gl_Position = Projection * wp;
    vcolor = color0 / 255.0;
    vnormal = normal.xyz;
    vpos = wp.xyz;
    vemis = texcoord.x;
}
"#;

const WORLD_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 vcolor;
varying mediump vec3 vnormal;
varying highp vec3 vpos;
varying lowp float vemis;
uniform vec3 Eye;
uniform vec4 Env;      // x: glow pulse 0..1, y: seconds, z: fog density, w: exposure
uniform vec3 FogColor;
uniform vec3 AmbientSky;
uniform vec3 AmbientGround;
uniform vec3 KeyDir;
uniform vec3 KeyColor;
uniform vec4 Point0;
uniform vec4 Point1;
uniform vec4 Point2;
uniform vec4 Point3;
uniform vec4 Color0;
uniform vec4 Color1;
uniform vec4 Color2;
uniform vec4 Color3;
uniform vec4 Rim;      // rgb colour, w strength
vec3 localLight(vec4 source, vec4 color, vec3 n) {
    vec3 delta = source.xyz - vpos;
    float d = length(delta);
    float falloff = clamp(1.0 - d / max(source.w, 0.0001), 0.0, 1.0);
    float diffuse = max(dot(n, delta / max(d, 0.0001)), 0.0);
    return color.rgb * color.w * diffuse * falloff * falloff;
}
void main() {
    vec3 n = normalize(vnormal);
    vec3 v = normalize(Eye - vpos);
    float diff = max(dot(n, normalize(KeyDir)), 0.0);
    vec3 amb = mix(AmbientGround, AmbientSky, 0.5 + 0.5 * n.y);
    vec3 local = localLight(Point0, Color0, n) + localLight(Point1, Color1, n)
        + localLight(Point2, Color2, n) + localLight(Point3, Color3, n);
    vec3 lit = vcolor.rgb * (amb + KeyColor * diff + local);
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    lit += Rim.rgb * rim * Rim.w;
    lit += vcolor.rgb * vemis * (0.85 + 0.85 * Env.x);
    // Soft tone map: keeps saturated colours from clipping to white.
    lit = vec3(1.0) - exp(-lit * 1.55);
    float d = length(Eye - vpos);
    float f = clamp(1.0 - exp(-d * Env.z), 0.0, 0.93);
    vec3 col = mix(lit, FogColor, f);
    gl_FragColor = vec4(col * Env.w, vcolor.a);
}
"#;

const FX_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 vcolor;
varying mediump vec3 vnormal;
varying highp vec3 vpos;
varying lowp float vemis;
uniform vec3 Eye;
uniform vec4 Env;
uniform vec3 FogColor;
void main() {
    float d = length(Eye - vpos);
    float f = clamp(1.0 - exp(-d * Env.z * 0.6), 0.0, 0.6);
    vec3 col = vcolor.rgb * (1.0 + vemis * 1.5);
    gl_FragColor = vec4(mix(col, FogColor, f * 0.5), vcolor.a);
}
"#;

const SKY_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
varying lowp vec4 vcolor;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    vcolor = color0 / 255.0;
}
"#;

const SKY_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 vcolor;
void main() { gl_FragColor = vcolor; }
"#;

/// Uniform names and types of the world and effect materials; kept in one list so the GLSL above and
/// the Rust that fills it cannot drift apart (a test compares them).
const WORLD_UNIFORMS: [(&str, UniformType); 16] = [
    ("Eye", UniformType::Float3),
    ("Env", UniformType::Float4),
    ("FogColor", UniformType::Float3),
    ("AmbientSky", UniformType::Float3),
    ("AmbientGround", UniformType::Float3),
    ("KeyDir", UniformType::Float3),
    ("KeyColor", UniformType::Float3),
    ("Rim", UniformType::Float4),
    ("Point0", UniformType::Float4),
    ("Point1", UniformType::Float4),
    ("Point2", UniformType::Float4),
    ("Point3", UniformType::Float4),
    ("Color0", UniformType::Float4),
    ("Color1", UniformType::Float4),
    ("Color2", UniformType::Float4),
    ("Color3", UniformType::Float4),
];

/// Uniforms the effect shader actually declares (a material must not declare more than its shader has).
const FX_UNIFORMS: [&str; 3] = ["Eye", "Env", "FogColor"];

fn uniforms(only: Option<&[&str]>) -> Vec<UniformDesc> {
    WORLD_UNIFORMS
        .iter()
        .filter(|(name, _)| only.is_none_or(|names| names.contains(name)))
        .map(|(name, kind)| UniformDesc::new(name, *kind))
        .collect()
}

fn alpha_blend() -> Option<BlendState> {
    Some(BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
    ))
}

fn additive_blend() -> Option<BlendState> {
    Some(BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        BlendFactor::One,
    ))
}

/// The four materials. Load once after the window exists (`Materials::load()` needs the GL context),
/// then each frame call [`Materials::set_scene`] and draw in this order: sky, world, `fx_alpha`,
/// `fx_add`, each with `gl_use_material(&materials.world)` and so on.
pub struct Materials {
    /// Opaque, lit, fogged geometry.
    pub world: Material,
    /// Blended geometry that does not write depth, for translucent surfaces. It is also **not depth-tested**
    /// (miniquad ignores `depth_test` when `depth_write` is false), so nearer opaque geometry does not hide it:
    /// see the module documentation.
    pub fx_alpha: Material,
    /// Additive glow: sparks, beams, shockwaves. Not depth-tested, like `fx_alpha`.
    pub fx_add: Material,
    /// Unlit gradient sky, drawn first without depth.
    pub sky: Material,
}

impl Materials {
    /// Compile the shaders. Fails only when the GL driver rejects them.
    pub fn load() -> Result<Self, macroquad::Error> {
        let world = load_material(
            ShaderSource::Glsl {
                vertex: WORLD_VERTEX,
                fragment: WORLD_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    depth_test: Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                uniforms: uniforms(None),
                ..Default::default()
            },
        )?;
        let fx = |blend| {
            load_material(
                ShaderSource::Glsl {
                    vertex: WORLD_VERTEX,
                    fragment: FX_FRAGMENT,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        // Ignored by miniquad 0.4.8 while depth_write is false (it disables the depth
                        // test): kept so the intent survives a backend that honours it. See module docs.
                        depth_test: Comparison::LessOrEqual,
                        depth_write: false,
                        color_blend: blend,
                        ..Default::default()
                    },
                    uniforms: uniforms(Some(&FX_UNIFORMS)),
                    ..Default::default()
                },
            )
        };
        let sky = load_material(
            ShaderSource::Glsl {
                vertex: SKY_VERTEX,
                fragment: SKY_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    depth_test: Comparison::Always,
                    depth_write: false,
                    color_blend: alpha_blend(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        Ok(Self {
            world,
            fx_alpha: fx(alpha_blend())?,
            fx_add: fx(additive_blend())?,
            sky,
        })
    }

    /// Bind the `world` material and draw every mesh: the common case for a scene's static geometry
    /// (built once with [`Template`](super::Template)/[`Batch`](super::Batch), unlike per-frame dynamic
    /// content, which goes through a `Batch` and `Batch::draw`). Equivalent to
    /// `gl_use_material(&materials.world)` followed by `draw_mesh` on each, except that it cannot be
    /// called with those two steps in the wrong order — which compiles, runs and renders nothing, with
    /// no error (a real bug caught only by inspecting a headless capture while building a game on this).
    pub fn draw_static(&self, meshes: &[Mesh]) {
        gl_use_material(&self.world);
        for mesh in meshes {
            draw_mesh(mesh);
        }
    }

    /// Set at most four local lights after `set_scene`, separately for each camera pass.
    /// Unused slots are cleared. An oversized list is rejected without changing uniforms.
    /// `set_scene` resets all local lights, preserving the appearance of existing kit clients.
    pub fn set_point_lights(&self, lights: &[PointLight]) -> Result<(), &'static str> {
        if lights.len() > MAX_POINT_LIGHTS {
            return Err("kit supports at most four point lights per pass");
        }
        for (i, (source, color)) in [
            ("Point0", "Color0"),
            ("Point1", "Color1"),
            ("Point2", "Color2"),
            ("Point3", "Color3"),
        ]
        .into_iter()
        .enumerate()
        {
            let (position, radiance) = lights.get(i).map_or((Vec4::ZERO, Vec4::ZERO), |l| {
                (l.position.extend(l.radius), l.color.extend(l.intensity))
            });
            self.world.set_uniform(source, position);
            self.world.set_uniform(color, radiance);
        }
        Ok(())
    }

    /// Per-frame scene constants for the world and effect materials: the camera `eye`, `time` in
    /// seconds, a `pulse` (0-1) that brightens glowing surfaces (a beat, a charge-up) and the [`Look`].
    pub fn set_scene(&self, look: &Look, eye: Vec3, time: f32, pulse: f32) {
        for m in [&self.world, &self.fx_alpha, &self.fx_add] {
            m.set_uniform("Eye", eye);
            m.set_uniform("Env", vec4(pulse, time, look.fog_density, look.exposure));
            m.set_uniform("FogColor", Vec3::from(look.fog_color));
        }
        self.set_point_lights(&[])
            .expect("empty light list is valid");
        let m = &self.world;
        m.set_uniform("AmbientSky", Vec3::from(look.ambient_sky));
        m.set_uniform("AmbientGround", Vec3::from(look.ambient_ground));
        m.set_uniform("KeyDir", Vec3::from(look.key_direction));
        m.set_uniform("KeyColor", Vec3::from(look.key_color));
        m.set_uniform(
            "Rim",
            vec4(
                look.rim_color[0],
                look.rim_color[1],
                look.rim_color[2],
                look.rim_strength,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_light_falloff_is_local_and_faces_the_source() {
        let light = PointLight::new(Vec3::Y * 2., 4., [1., 0.5, 0.], 2.).unwrap();
        assert!((light.diffuse_at(Vec3::ZERO, Vec3::Y) - vec3(0.5, 0.25, 0.)).length() < 1e-6);
        assert_eq!(light.diffuse_at(Vec3::ZERO, -Vec3::Y), Vec3::ZERO);
        assert_eq!(light.diffuse_at(Vec3::Y * 6., -Vec3::Y), Vec3::ZERO);
        assert_eq!(light.diffuse_at(Vec3::Y * 2., Vec3::Y), Vec3::ZERO);
        assert_eq!(light.diffuse_at(Vec3::ZERO, Vec3::ZERO), Vec3::ZERO);
        assert!(PointLight::new(Vec3::ZERO, 0., [1.; 3], 1.).is_err());
        assert!(PointLight::new(Vec3::ZERO, 2., [-1., 1., 1.], 1.).is_err());
        assert!(PointLight::new(Vec3::ZERO, 2., [1.; 3], f32::NAN).is_err());
    }

    #[test]
    fn every_declared_uniform_is_used_by_the_world_shader_and_vice_versa() {
        for (name, kind) in WORLD_UNIFORMS {
            let glsl_type = match kind {
                UniformType::Float3 => "vec3",
                UniformType::Float4 => "vec4",
                other => panic!("unexpected uniform type {other:?}"),
            };
            let declaration = format!("uniform {glsl_type} {name};");
            assert!(
                WORLD_FRAGMENT.contains(&declaration),
                "world fragment shader lacks `{declaration}`"
            );
        }
        let declared = WORLD_FRAGMENT.matches("uniform ").count();
        assert_eq!(
            declared,
            WORLD_UNIFORMS.len(),
            "the shader declares a uniform the list does not know"
        );
        // The effect shader uses a subset of the same list (unused uniforms are allowed, undeclared are not).
        for line in FX_FRAGMENT.lines().filter(|l| l.starts_with("uniform ")) {
            let name = line
                .split_whitespace()
                .nth(2)
                .unwrap()
                .trim_end_matches(';');
            assert!(
                FX_UNIFORMS.contains(&name),
                "fx shader uniform {name} is not in FX_UNIFORMS"
            );
        }
        assert_eq!(FX_FRAGMENT.matches("uniform ").count(), FX_UNIFORMS.len());
    }

    #[test]
    fn presets_are_finite_in_range_and_visibly_different() {
        let looks = [Look::daylight(), Look::dusk(), Look::night()];
        for l in looks {
            let all = [
                l.ambient_sky,
                l.ambient_ground,
                l.key_color,
                l.rim_color,
                l.fog_color,
            ];
            assert!(all
                .iter()
                .flatten()
                .all(|c| c.is_finite() && (0. ..=1.).contains(c)));
            assert!(l.key_direction.iter().all(|c| c.is_finite()) && l.key_direction != [0.; 3]);
            assert!(l.fog_density > 0. && l.exposure == 1. && l.rim_strength >= 0.);
        }
        let mean = |c: Rgb| (c[0] + c[1] + c[2]) / 3.;
        assert!(mean(looks[0].ambient_sky) > mean(looks[1].ambient_sky));
        assert!(
            mean(looks[1].ambient_sky) > mean(looks[2].ambient_sky),
            "night is the darkest"
        );
        assert_eq!(Look::default(), Look::daylight());
        let clear = Look::night().clear_color();
        assert_eq!((clear.r, clear.a), (Look::night().fog_color[0], 1.));
    }
}
