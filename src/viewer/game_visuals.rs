//! Reusable, cached world materials and recognizable first-person weapon silhouettes.
use crate::geometry::World;
use crate::math::{Ray, V};
use macroquad::prelude::*;
use std::collections::VecDeque;

pub struct SurfaceRenderer {
    material: Material,
    sign_material: Material,
    signs: Vec<Mesh>,
}
impl SurfaceRenderer {
    pub fn new() -> Result<Self, macroquad::Error> {
        Ok(Self {
            signs: Vec::new(),
            // Glyph tiles overlap. Transparent pixels must never write depth and
            // randomly erase neighboring letters as the camera moves.
            sign_material: load_material(
                ShaderSource::Glsl {
                    vertex: SIGN_VERTEX,
                    fragment: SIGN_FRAGMENT,
                },
                MaterialParams {
                    pipeline_params: sign_pipeline(),
                    ..Default::default()
                },
            )?,
            material: load_material(
                ShaderSource::Glsl {
                    vertex: WORLD_VERTEX,
                    fragment: WORLD_FRAGMENT,
                },
                MaterialParams {
                    pipeline_params: miniquad::PipelineParams {
                        depth_test: miniquad::Comparison::LessOrEqual,
                        depth_write: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?,
        })
    }
    pub fn add_sign(&mut self, text: &str, origin: Vec3, right: Vec3, height: f32) {
        self.signs.push(super::game_text::sign_mesh(
            text,
            origin,
            right,
            Vec3::Y,
            height,
            Color::new(0.12, 0.17, 0.18, 1.),
        ));
    }
    pub fn draw(&self, meshes: &[Mesh]) {
        gl_use_material(&self.material);
        for mesh in meshes {
            draw_mesh(mesh);
        }
        gl_use_material(&self.sign_material);
        for sign in &self.signs {
            draw_mesh(sign);
        }
        gl_use_default_material();
    }
}
fn sign_pipeline() -> miniquad::PipelineParams {
    miniquad::PipelineParams {
        depth_test: miniquad::Comparison::LessOrEqual,
        depth_write: false,
        color_blend: Some(miniquad::BlendState::new(
            miniquad::Equation::Add,
            miniquad::BlendFactor::Value(miniquad::BlendValue::SourceAlpha),
            miniquad::BlendFactor::OneMinusValue(miniquad::BlendValue::SourceAlpha),
        )),
        ..Default::default()
    }
}
const SIGN_VERTEX: &str = r#"#version 100
attribute vec3 position; attribute vec2 texcoord; attribute vec4 color0;
uniform mat4 Model; uniform mat4 Projection;
varying mediump vec2 uv; varying lowp vec4 tint;
void main(){gl_Position=Projection*Model*vec4(position,1.);uv=texcoord;tint=color0/255.;}
"#;
const SIGN_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
varying mediump vec2 uv; varying lowp vec4 tint;
void main(){vec4 c=texture2D(Texture,uv)*tint;if(c.a<0.001)discard;gl_FragColor=c;}
"#;

#[cfg(test)]
mod sign_tests {
    #[test]
    fn overlapping_letters_do_not_occlude_each_other_but_walls_do() {
        let pipeline = super::sign_pipeline();
        assert!(!pipeline.depth_write);
        assert_eq!(
            pipeline.depth_test,
            macroquad::miniquad::Comparison::LessOrEqual
        );
        assert!(pipeline.color_blend.is_some());
    }
}
const WORLD_VERTEX: &str = r#"#version 100
attribute vec3 position;attribute vec2 texcoord;attribute vec4 color0;attribute vec4 normal;
uniform mat4 Model;uniform mat4 Projection;
varying highp vec3 p;varying mediump vec3 n;varying lowp vec4 c;varying mediump float rough;
void main(){p=position;n=normal.xyz;c=color0/255.;rough=texcoord.y;gl_Position=Projection*Model*vec4(position,1.);}
"#;
const WORLD_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec3 p;varying mediump vec3 n;varying lowp vec4 c;varying mediump float rough;
float hash(vec2 q){return fract(sin(dot(q,vec2(127.1,311.7)))*43758.5453);}
float noise(vec2 q){vec2 i=floor(q);vec2 f=fract(q);f=f*f*(3.-2.*f);return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1,1)),f.x),f.y);}
void main(){vec3 a=abs(n);vec2 uv=a.y>0.7?p.xz:(a.x>0.7?p.zy:p.xy);
 float grain=noise(uv*18.);float mottling=noise(uv*3.);float shade=0.88+0.08*mottling+0.06*grain;
 if(rough>0.9 && rough<0.96 && a.y<0.7){vec2 b=uv/vec2(0.65,0.24);b.x+=mod(floor(b.y),2.)*0.5;vec2 f=fract(b);float mortar=max(step(f.x,0.035),step(f.y,0.065));shade=mix(0.78+0.20*noise(floor(b)),1.2,mortar);}
 else if(rough<0.72){shade*=0.92+0.08*smoothstep(0.1,0.35,abs(sin(uv.x*35.)));}
 else {vec2 grid=fract(uv/vec2(3.,1.5));float seam=max(1.-smoothstep(0.002,0.012,grid.x),1.-smoothstep(0.003,0.012,grid.y));shade*=1.-0.18*seam;}
 if(a.y<0.7)shade*=0.79+0.21*smoothstep(0.,1.3,p.y);
 gl_FragColor=vec4(c.rgb*shade,1.);}
"#;

#[derive(Clone, Copy)]
pub enum WeaponStyle {
    Pistol,
    Rifle,
    Shotgun,
    Scoped,
    Heavy,
    Launcher,
}
impl WeaponStyle {
    fn index(self) -> usize {
        self as usize
    }
}
/// Models are built once. Shot effects are cosmetic; damage remains server-owned.
pub struct WeaponPresentation {
    models: Vec<Mesh>,
    pub last_shot: f64,
    next_shot: f64,
    impacts: VecDeque<(f64, V, V)>,
    sound: Option<macroquad::audio::Sound>,
}
impl WeaponPresentation {
    pub async fn new() -> Self {
        let sound =
            macroquad::audio::load_sound_from_bytes(include_bytes!("../../assets/ui/shot.wav"))
                .await
                .ok();
        Self {
            models: (0..6).map(weapon_mesh).collect(),
            last_shot: -10.,
            next_shot: 0.,
            impacts: VecDeque::new(),
            sound,
        }
    }
    /// Client-side immediate feedback, rate limited to the active weapon's cadence.
    pub fn trigger(&mut self, held: bool, seconds: f64, origin: V, direction: V, world: &World) {
        let now = get_time();
        if !held || now < self.next_shot {
            return;
        }
        self.last_shot = now;
        self.next_shot = now + seconds;
        let ray = Ray {
            o: origin,
            d: direction,
        };
        let end = world
            .hit(ray, 150., false)
            .map_or(origin + direction * 150., |h| {
                origin + direction * (h.t - 0.015)
            });
        self.impacts.push_back((now, origin, end));
        while self.impacts.len() > 24 {
            self.impacts.pop_front();
        }
        if let Some(s) = &self.sound {
            macroquad::audio::play_sound(
                s,
                macroquad::audio::PlaySoundParams {
                    looped: false,
                    volume: 0.28,
                },
            );
        }
    }
    pub fn world_effects(&self) {
        let now = get_time();
        for &(t, start, end) in &self.impacts {
            let age = now - t;
            if age < 0.065 {
                draw_line_3d(
                    vec3(start.0, start.1 - 0.12, start.2),
                    vec3(end.0, end.1, end.2),
                    Color::new(1., 0.77, 0.28, 1.),
                );
            }
            if age < 3. {
                draw_sphere(
                    vec3(end.0, end.1, end.2),
                    if age < 0.08 { 0.07 } else { 0.025 },
                    None,
                    if age < 0.08 {
                        ORANGE
                    } else {
                        Color::new(0.12, 0.1, 0.08, 1.)
                    },
                );
            }
        }
    }
    pub fn draw(&self, style: WeaponStyle, aim: f32, moving: bool, reloading: bool) {
        let age = (get_time() - self.last_shot) as f32;
        let kick = if age < 0.22 {
            (1. - age / 0.22).powi(2)
        } else {
            0.
        };
        let sway = if moving {
            (get_time() as f32 * 9.).sin() * 0.008
        } else {
            0.
        };
        let drop = if reloading { 0.16 } else { 0. };
        set_camera(&Camera3D {
            position: vec3(aim * 0.21, sway + drop, -kick * 0.055),
            target: vec3(aim * 0.21, sway + drop - 0.015, -1. - kick * 0.055),
            up: Vec3::Y,
            fovy: 58_f32.to_radians(),
            z_near: 0.1,
            z_far: 10.,
            ..Default::default()
        });
        draw_mesh(&self.models[style.index()]);
        if age < 0.045 {
            let m = vec3(0.22, -0.155, -1.40);
            draw_sphere(m, 0.047, None, YELLOW);
            draw_sphere(m + vec3(0., 0., -0.09), 0.028, None, ORANGE);
        }
        set_default_camera();
    }
}
fn weapon_mesh(kind: usize) -> Mesh {
    let mut m = Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    };
    let steel = Color::new(0.31, 0.33, 0.34, 1.);
    let dark = Color::new(0.14, 0.15, 0.15, 1.);
    let polymer = Color::new(0.24, 0.24, 0.19, 1.);
    let glove = Color::new(0.27, 0.29, 0.24, 1.);
    let pistol = kind == 0;
    let barrel = if pistol { -0.73 } else { -1.02 };
    // Receiver, stock, pistol grip, magazine and distinct forward handguard.
    cuboid(
        &mut m,
        vec3(0.22, -0.20, -0.55),
        vec3(0.095, 0.12, if pistol { 0.23 } else { 0.35 }),
        steel,
    );
    cuboid(
        &mut m,
        vec3(0.22, -0.28, -0.40),
        vec3(0.075, 0.20, 0.10),
        polymer,
    );
    if !pistol {
        cuboid(
            &mut m,
            vec3(0.22, -0.23, -0.23),
            vec3(0.11, 0.16, 0.22),
            polymer,
        );
        cuboid(
            &mut m,
            vec3(0.22, -0.21, -0.79),
            vec3(0.085, 0.10, 0.25),
            polymer,
        );
        cuboid(
            &mut m,
            vec3(0.22, -0.32, -0.61),
            vec3(if kind == 4 { 0.17 } else { 0.065 }, 0.20, 0.12),
            dark,
        );
        for i in 0..7 {
            cuboid(
                &mut m,
                vec3(0.22, -0.148, -0.68 - i as f32 * 0.032),
                vec3(0.095, 0.015, 0.012),
                steel,
            );
        }
    }
    tube(
        &mut m,
        vec3(0.22, -0.18, barrel),
        if kind == 5 { 0.082 } else { 0.021 },
        if pistol { 0.20 } else { 0.33 },
        steel,
    );
    tube(
        &mut m,
        vec3(0.22, -0.18, barrel - 0.17),
        if kind == 5 { 0.069 } else { 0.024 },
        0.06,
        dark,
    );
    if kind == 2 {
        tube(&mut m, vec3(0.22, -0.23, -0.92), 0.021, 0.30, steel);
    }
    if kind == 3 {
        tube(&mut m, vec3(0.22, -0.086, -0.59), 0.044, 0.25, dark);
        tube(
            &mut m,
            vec3(0.22, -0.086, -0.44),
            0.036,
            0.02,
            Color::new(0.14, 0.3, 0.33, 1.),
        );
    } else {
        cuboid(
            &mut m,
            vec3(0.22, -0.112, -0.43),
            vec3(0.066, 0.045, 0.028),
            dark,
        );
        cuboid(
            &mut m,
            vec3(0.22, -0.117, -0.89),
            vec3(0.016, 0.04, 0.025),
            dark,
        );
    }
    // Gloved hands and sleeves under the grip/fore-end keep a human scale.
    cuboid(
        &mut m,
        vec3(0.27, -0.36, -0.34),
        vec3(0.12, 0.13, 0.19),
        glove,
    );
    cuboid(
        &mut m,
        vec3(0.34, -0.44, -0.17),
        vec3(0.15, 0.18, 0.25),
        Color::new(0.32, 0.34, 0.28, 1.),
    );
    if !pistol {
        cuboid(
            &mut m,
            vec3(0.17, -0.29, -0.77),
            vec3(0.15, 0.12, 0.17),
            glove,
        );
        cuboid(
            &mut m,
            vec3(0.10, -0.40, -0.59),
            vec3(0.14, 0.14, 0.30),
            Color::new(0.32, 0.34, 0.28, 1.),
        );
    }
    for v in &mut m.vertices {
        v.position.z -= 0.30;
    }
    m
}
fn cuboid(m: &mut Mesh, center: Vec3, size: Vec3, c: Color) {
    let hx = size.x * 0.5;
    let hy = size.y * 0.5;
    let bevel = hx.min(hy) * 0.22;
    let profile = [
        vec2(-hx + bevel, -hy),
        vec2(hx - bevel, -hy),
        vec2(hx, -hy + bevel),
        vec2(hx, hy - bevel),
        vec2(hx - bevel, hy),
        vec2(-hx + bevel, hy),
        vec2(-hx, hy - bevel),
        vec2(-hx, -hy + bevel),
    ];
    for i in 0..8 {
        let a = profile[i];
        let b = profile[(i + 1) % 8];
        let n = vec3(b.y - a.y, a.x - b.x, 0.).normalize();
        let lit = 0.55 + 0.45 * n.dot(vec3(-0.4, 0.8, 0.5).normalize()).max(0.);
        let color = Color::new(c.r * lit, c.g * lit, c.b * lit, 1.);
        let base = m.vertices.len() as u16;
        for (p, z) in [(a, -1.), (b, -1.), (b, 1.), (a, 1.)] {
            m.vertices.push(Vertex::new2(
                center + vec3(p.x, p.y, z * size.z * 0.5),
                Vec2::ZERO,
                color,
            ));
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    for z in [-1., 1.] {
        let base = m.vertices.len() as u16;
        let color = Color::new(c.r * 0.8, c.g * 0.8, c.b * 0.8, 1.);
        m.vertices.push(Vertex::new2(
            center + vec3(0., 0., z * size.z * 0.5),
            Vec2::ZERO,
            color,
        ));
        for p in profile {
            m.vertices.push(Vertex::new2(
                center + vec3(p.x, p.y, z * size.z * 0.5),
                Vec2::ZERO,
                color,
            ));
        }
        for i in 0..8 {
            m.indices
                .extend([base, base + 1 + i, base + 1 + (i + 1) % 8]);
        }
    }
}
fn tube(m: &mut Mesh, center: Vec3, r: f32, length: f32, color: Color) {
    for i in 0..16 {
        let a = i as f32 / 16. * std::f32::consts::TAU;
        let b = (i + 1) as f32 / 16. * std::f32::consts::TAU;
        let n = vec3(a.cos(), a.sin(), 0.);
        let lit = 0.5 + 0.5 * n.dot(vec3(-0.4, 0.8, 0.5).normalize()).max(0.);
        let c = Color::new(color.r * lit, color.g * lit, color.b * lit, 1.);
        let base = m.vertices.len() as u16;
        for (angle, z) in [(a, -1.), (b, -1.), (b, 1.), (a, 1.)] {
            m.vertices.push(Vertex::new2(
                center + vec3(angle.cos() * r, angle.sin() * r, z * length * 0.5),
                Vec2::ZERO,
                c,
            ));
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
