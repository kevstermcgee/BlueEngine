//! Scene lighting and the materials that draw dynamic geometry.
//!
//! The stock material (`mesh::material`) is tuned for baked house interiors. A game whose look is
//! neon, a desert or a night sky wants its own light, fog and glow, and the smallest way to get them is
//! four ready-made materials driven by one plain-data [`Look`]:
//!
//! | Material | Use |
//! |---|---|
//! | `world` | opaque, lit by a hemisphere ambient, one key light and a rim, glow, tone-mapped, fogged |
//! | `fx_alpha` | translucent surfaces that do not write depth (shadows blobs, glass, halos) |
//! | `fx_add` | additive glow: sparks, beams, shockwaves |
//! | `sky` | unlit vertex-coloured dome, drawn first without depth |
//!
//! Geometry comes from [`Template`](super::Template)/[`Batch`](super::Batch) (vertex `uv.x` is glow,
//! `normal.xyz` the normal) or from `mesh::bake_with` for a static world.
use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams, UniformDesc,
    UniformType,
};
use macroquad::prelude::*;

use super::batch::Rgb;

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
uniform vec4 Rim;      // rgb colour, w strength
void main() {
    vec3 n = normalize(vnormal);
    vec3 v = normalize(Eye - vpos);
    float diff = max(dot(n, normalize(KeyDir)), 0.0);
    vec3 amb = mix(AmbientGround, AmbientSky, 0.5 + 0.5 * n.y);
    vec3 lit = vcolor.rgb * (amb + KeyColor * diff);
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
const WORLD_UNIFORMS: [(&str, UniformType); 8] = [
    ("Eye", UniformType::Float3),
    ("Env", UniformType::Float4),
    ("FogColor", UniformType::Float3),
    ("AmbientSky", UniformType::Float3),
    ("AmbientGround", UniformType::Float3),
    ("KeyDir", UniformType::Float3),
    ("KeyColor", UniformType::Float3),
    ("Rim", UniformType::Float4),
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
    /// Blended geometry that does not write depth, for translucent surfaces.
    pub fx_alpha: Material,
    /// Additive glow: sparks, beams, shockwaves.
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

    /// Per-frame scene constants for the world and effect materials: the camera `eye`, `time` in
    /// seconds, a `pulse` (0-1) that brightens glowing surfaces (a beat, a charge-up) and the [`Look`].
    pub fn set_scene(&self, look: &Look, eye: Vec3, time: f32, pulse: f32) {
        for m in [&self.world, &self.fx_alpha, &self.fx_add] {
            m.set_uniform("Eye", eye);
            m.set_uniform("Env", vec4(pulse, time, look.fog_density, look.exposure));
            m.set_uniform("FogColor", Vec3::from(look.fog_color));
        }
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
