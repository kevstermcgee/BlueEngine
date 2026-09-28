//! Tessellate Vesper's evaluated primitives once, baking visibility with its BVH.
//! GPU frames reuse these meshes; no scene rebuild or ray tracing in the frame loop.
use crate::{
    geometry::{Instance, Primitive, World},
    math::{Ray, V},
};
use macroquad::prelude::*;

pub fn vec(v: V) -> Vec3 {
    vec3(v.0, v.1, v.2)
}
/// A point light for baked shading. Falloff is `power * 0.25 / (3 + distance^2)`; each light is
/// sampled at four points `Lighting::light_radius` apart so shadow edges are soft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: V,
    /// Linear RGB, 0-1 per channel.
    pub color: V,
    pub power: f32,
}

/// How static geometry is lit when it is baked into vertex colours. [`Lighting::house`] is the look
/// the stock client and `bake_tagged` have always used (a furnished interior with three ceiling lights,
/// contact shadows and ambient occlusion); build another for a different game.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Ambient light colour, scaled by `0.8 + 0.2 * normal.y` (a little brighter facing up).
    pub ambient: V,
    pub lights: Vec<PointLight>,
    /// Spread of the four sample points of each light, in metres.
    pub light_radius: f32,
    /// Trace a shadow ray from every vertex to every light sample: the expensive part of a bake.
    pub shadows: bool,
    /// Darken creases and corners by sampling occlusion around every vertex.
    pub ambient_occlusion: bool,
    /// Direction *towards* the light for the flat shading of tagged "simple" surfaces (tag above 2.5).
    pub flat_key: V,
    /// Flat shading is `flat_base + flat_gain * max(0, normal . flat_key)`.
    pub flat_base: f32,
    pub flat_gain: f32,
}

impl Lighting {
    /// The furnished-interior look: cool ambient, three ceiling lights with contact shadows and ambient
    /// occlusion. Baking with it is identical to `bake_tagged`.
    pub fn house() -> Self {
        Self {
            ambient: V(0.24, 0.27, 0.31),
            lights: vec![
                PointLight {
                    position: V(-4.8, 3., 1.0),
                    color: V(0.72, 0.87, 1.),
                    power: 18.,
                },
                PointLight {
                    position: V(2.4, 3.25, -2.5),
                    color: V(1., 0.86, 0.66),
                    power: 15.,
                },
                PointLight {
                    position: V(2.4, 3.25, 3.),
                    color: V(0.82, 0.91, 1.),
                    power: 10.,
                },
            ],
            light_radius: 0.18,
            shadows: true,
            ambient_occlusion: true,
            flat_key: V(-0.4, 0.8, -0.3),
            flat_base: 0.55,
            flat_gain: 0.45,
        }
    }

    /// Outdoor sun: one distant light in `direction_to_sun` with hard shadows and a sky-tinted ambient.
    /// `intensity` around 1 is a bright day. Shadows and occlusion are traced, so bake once at load.
    pub fn sun(direction_to_sun: V, color: V, intensity: f32) -> Self {
        let distance = 400.;
        let d = direction_to_sun.norm();
        // Irradiance at the scene is power * 0.25 / (3 + distance^2); solve for the power.
        let power = intensity * 4. * (3. + distance * distance);
        Self {
            ambient: V(0.30, 0.36, 0.46),
            lights: vec![PointLight {
                position: d * distance,
                color,
                power,
            }],
            light_radius: 0.05,
            shadows: true,
            ambient_occlusion: true,
            flat_key: d,
            flat_base: 0.55,
            flat_gain: 0.45,
        }
    }

    /// Uniform, shadowless light: every surface shows its colour times `ambient`, lit slightly from
    /// above. Cheapest to bake, and right for stylised games whose renderer adds its own light.
    pub fn flat(ambient: V) -> Self {
        Self {
            ambient,
            lights: Vec::new(),
            light_radius: 0.,
            shadows: false,
            ambient_occlusion: false,
            flat_key: V(-0.4, 0.8, -0.3),
            flat_base: 0.55,
            flat_gain: 0.45,
        }
    }
}

pub fn bake(world: &World) -> Vec<Mesh> {
    bake_tagged(world, &[])
}
/// Bake with the house lighting. Use [`bake_with`] for any other look.
pub fn bake_tagged(world: &World, tags: &[(super::controller::Collider, f32)]) -> Vec<Mesh> {
    bake_with(world, &Lighting::house(), tags)
}

/// Bake static geometry with a chosen [`Lighting`]; `tags` mark regions for the stock shader's
/// special cases exactly as for `bake_tagged`.
pub fn bake_with(
    world: &World,
    lighting: &Lighting,
    tags: &[(super::controller::Collider, f32)],
) -> Vec<Mesh> {
    bake_tagged_entities_with(world, lighting, tags, &[]).0
}

/// Bake selected semantic boxes separately so a client can change their presentation
/// without rebuilding the static world. Entity order in the result matches `entities`.
pub fn bake_tagged_entities(
    world: &World,
    tags: &[(super::controller::Collider, f32)],
    entity_bounds: &[super::controller::Collider],
) -> (Vec<Mesh>, Vec<Vec<Mesh>>) {
    bake_tagged_entities_with(world, &Lighting::house(), tags, entity_bounds)
}

/// [`bake_tagged_entities`] with a chosen [`Lighting`].
pub fn bake_tagged_entities_with(
    world: &World,
    lighting: &Lighting,
    tags: &[(super::controller::Collider, f32)],
    entity_bounds: &[super::controller::Collider],
) -> (Vec<Mesh>, Vec<Vec<Mesh>>) {
    let flat_key = lighting.flat_key.norm();
    let new_group = || {
        vec![Mesh {
            vertices: vec![],
            indices: vec![],
            texture: None,
        }]
    };
    let mut groups: Vec<Vec<Mesh>> = (0..=entity_bounds.len()).map(|_| new_group()).collect();
    for instance in &world.instances {
        let same_bounds = |bounds: &super::controller::Collider| {
            (instance.bounds.lo - bounds.min).length() < 0.001
                && (instance.bounds.hi - bounds.max).length() < 0.001
        };
        let group = entity_bounds
            .iter()
            .position(same_bounds)
            .map_or(0, |index| index + 1);
        let meshes = &mut groups[group];
        let center = (instance.bounds.lo + instance.bounds.hi) * 0.5;
        let tag = tags
            .iter()
            .find(|(b, _)| b.contains(center))
            .map_or(0., |(_, tag)| *tag);
        let simple = tag > 2.5;
        let transform = instance.inverse.inverse();
        let mut lighting_cache = std::collections::HashMap::new();
        let mut triangle = |positions: [V; 3], normals: [V; 3]| {
            if meshes.last().unwrap().vertices.len() + 3 > 9000 {
                meshes.push(Mesh {
                    vertices: vec![],
                    indices: vec![],
                    texture: None,
                });
            }
            let mesh = meshes.last_mut().unwrap();
            for k in 0..3 {
                let p = transform.point(positions[k]);
                let n = instance.inverse.normal_from_inverse(normals[k]);
                let key = [
                    p.0.to_bits(),
                    p.1.to_bits(),
                    p.2.to_bits(),
                    n.0.to_bits(),
                    n.1.to_bits(),
                    n.2.to_bits(),
                ];
                let c = *lighting_cache.entry(key).or_insert_with(|| {
                    if simple {
                        let light =
                            lighting.flat_base + lighting.flat_gain * n.dot(flat_key).max(0.);
                        let c = instance.material.color * light;
                        V(c.0.sqrt(), c.1.sqrt(), c.2.sqrt())
                    } else {
                        shade(world, lighting, instance, p, n)
                    }
                });
                let mut vertex = Vertex::new2(
                    vec(p),
                    vec2(tag, instance.material.roughness),
                    Color::new(c.0, c.1, c.2, 1.),
                );
                vertex.normal = vec4(n.0, n.1, n.2, instance.material.metallic);
                mesh.indices.push(mesh.vertices.len() as u16);
                mesh.vertices.push(vertex);
            }
        };
        match &instance.shape {
            Primitive::Triangle(p, n) => triangle(*p, *n),
            Primitive::Box => {
                for (normal, u, v) in [
                    (V(1., 0., 0.), V(0., 0., 1.), V(0., 1., 0.)),
                    (V(-1., 0., 0.), V(0., 0., 1.), V(0., 1., 0.)),
                    (V(0., 1., 0.), V(1., 0., 0.), V(0., 0., 1.)),
                    (V(0., -1., 0.), V(1., 0., 0.), V(0., 0., 1.)),
                    (V(0., 0., 1.), V(1., 0., 0.), V(0., 1., 0.)),
                    (V(0., 0., -1.), V(1., 0., 0.), V(0., 1., 0.)),
                ] {
                    let nu = (transform.vector(u).length() * 2. / 0.38)
                        .ceil()
                        .clamp(1., 40.) as usize;
                    let nv = (transform.vector(v).length() * 2. / 0.38)
                        .ceil()
                        .clamp(1., 40.) as usize;
                    let (nu, nv) = if simple { (1, 1) } else { (nu, nv) };
                    for i in 0..nu {
                        for j in 0..nv {
                            let p = |a: usize, b: usize| {
                                normal
                                    + u * (a as f32 / nu as f32 * 2. - 1.)
                                    + v * (b as f32 / nv as f32 * 2. - 1.)
                            };
                            triangle([p(i, j), p(i + 1, j), p(i + 1, j + 1)], [normal; 3]);
                            triangle([p(i, j), p(i + 1, j + 1), p(i, j + 1)], [normal; 3]);
                        }
                    }
                }
            }
            Primitive::Sphere => {
                let (slices, rings) = if simple { (12, 6) } else { (24, 12) };
                let p = |i: usize, j: usize| {
                    let a = i as f32 / slices as f32 * std::f32::consts::TAU;
                    let b = j as f32 / rings as f32 * std::f32::consts::PI;
                    V(a.cos() * b.sin(), b.cos(), a.sin() * b.sin())
                };
                for i in 0..slices {
                    for j in 0..rings {
                        let a = [p(i, j), p(i + 1, j), p(i + 1, j + 1)];
                        let b = [p(i, j), p(i + 1, j + 1), p(i, j + 1)];
                        triangle(a, a);
                        triangle(b, b);
                    }
                }
            }
            Primitive::Cylinder | Primitive::Cone => {
                let cone = matches!(instance.shape, Primitive::Cone);
                for i in 0..24 {
                    let a = i as f32 / 24. * std::f32::consts::TAU;
                    let b = (i + 1) as f32 / 24. * std::f32::consts::TAU;
                    let bottom_a = V(a.cos(), -1., a.sin());
                    let bottom_b = V(b.cos(), -1., b.sin());
                    let top_a = if cone {
                        V(0., 1., 0.)
                    } else {
                        V(a.cos(), 1., a.sin())
                    };
                    let top_b = if cone {
                        V(0., 1., 0.)
                    } else {
                        V(b.cos(), 1., b.sin())
                    };
                    let na = V(a.cos(), if cone { 0.5 } else { 0. }, a.sin()).norm();
                    let nb = V(b.cos(), if cone { 0.5 } else { 0. }, b.sin()).norm();
                    triangle([bottom_a, bottom_b, top_b], [na, nb, nb]);
                    if !cone {
                        triangle([bottom_a, top_b, top_a], [na, nb, na]);
                        triangle([V(0., 1., 0.), top_a, top_b], [V(0., 1., 0.); 3]);
                    }
                    triangle([V(0., -1., 0.), bottom_b, bottom_a], [V(0., -1., 0.); 3]);
                }
            }
        }
    }
    // Exact vertex sharing preserves normals, material tags and baked illumination.
    for meshes in &mut groups {
        for mesh in meshes {
            let mut seen = std::collections::HashMap::new();
            let mut unique: Vec<Vertex> = Vec::new();
            for index in &mut mesh.indices {
                let v = mesh.vertices[*index as usize];
                let key = (
                    v.position.to_array().map(f32::to_bits),
                    v.normal.to_array().map(f32::to_bits),
                    v.uv.to_array().map(f32::to_bits),
                    v.color,
                );
                *index = *seen.entry(key).or_insert_with(|| {
                    let i = unique.len() as u16;
                    unique.push(v);
                    i
                });
            }
            mesh.vertices = unique;
        }
    }
    let entity_groups = groups.split_off(1);
    (groups.pop().unwrap_or_default(), entity_groups)
}

fn shade(world: &World, lighting: &Lighting, instance: &Instance, p: V, n: V) -> V {
    let mat = &instance.material;
    if mat.emission > 0. {
        return (mat.color * (0.75 + mat.emission * 0.25)).min(V::ONE);
    }
    let origin = p + n * 0.012;
    let mut light = lighting.ambient * (0.8 + 0.2 * n.1);
    // Actual Vesper intersections provide static contact shadows.
    let r = lighting.light_radius;
    for PointLight {
        position: pos,
        color,
        power,
    } in &lighting.lights
    {
        for offset in [V(-r, 0., -r), V(r, 0., -r), V(-r, 0., r), V(r, 0., r)] {
            let delta = *pos + offset - origin;
            let distance = delta.length();
            let direction = delta / distance;
            let ndl = n.dot(direction).max(0.);
            if ndl > 0.
                && (!lighting.shadows
                    || world
                        .hit(
                            Ray {
                                o: origin,
                                d: direction,
                            },
                            distance - 0.025,
                            true,
                        )
                        .is_none())
            {
                light = light + *color * (ndl * *power * 0.25 / (3. + distance * distance));
            }
        }
    }

    let mut ao = 1.;
    if lighting.ambient_occlusion {
        let tangent = if n.1.abs() < 0.9 {
            n.cross(V(0., 1., 0.)).norm()
        } else {
            n.cross(V(1., 0., 0.)).norm()
        };
        let bitangent = n.cross(tangent);
        for d in [n, (n + tangent * 0.7).norm(), (n + bitangent * 0.7).norm()] {
            if let Some(h) = world.hit(Ray { o: origin, d }, 0.7, false) {
                ao -= 0.14 * (1. - h.t / 0.7);
            }
        }
    }
    let linear = mat.color * light * ao;
    V(
        linear.0.max(0.).powf(1. / 2.2),
        linear.1.max(0.).powf(1. / 2.2),
        linear.2.max(0.).powf(1. / 2.2),
    )
    .min(V::ONE)
}

pub fn material() -> Result<macroquad::material::Material, macroquad::Error> {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: true,
                cull_face: miniquad::CullFace::Nothing,
                ..Default::default()
            },
            uniforms: vec![
                UniformDesc::new("Eye", UniformType::Float3),
                UniformDesc::new("ObjectStates", UniformType::Float2),
            ],
            ..Default::default()
        },
    )
}
const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
varying lowp vec4 vcolor;
varying mediump vec3 vnormal;
varying mediump vec3 vpos;
varying lowp float metal;
varying lowp float tag;
varying lowp float roughness;
void main(){gl_Position=Projection*Model*vec4(position,1.0);vcolor=color0/255.0;vnormal=normal.xyz;vpos=position;metal=normal.w;tag=texcoord.x;roughness=texcoord.y;}
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 vcolor;
varying mediump vec3 vnormal;
varying mediump vec3 vpos;
varying lowp float metal;
uniform vec3 Eye;
uniform vec2 ObjectStates;
varying lowp float tag;
varying lowp float roughness;
void main(){
 vec3 n=normalize(vnormal);vec3 v=normalize(Eye-vpos);
 vec3 h=normalize(normalize(vec3(-3.0,5.0,2.0)-vpos)+v);
 float r=clamp(roughness,0.12,1.0);
 float fresnel=pow(1.0-max(dot(n,v),0.0),5.0);
 float spec=pow(max(dot(n,h),0.0),mix(96.0,8.0,r))*(0.035+metal*0.22)*(1.0-r*0.5);
 vec3 c=vcolor.rgb+vec3(spec)+vec3(0.12,0.18,0.25)*fresnel*metal;
 if(tag>0.5 && tag<1.5 && ObjectStates.x<0.5){c=vec3(0.015,0.024,0.035)+vec3(spec*0.2);}
 if(tag>1.5 && tag<2.5 && ObjectStates.y>0.5){float shade=max(vcolor.b,0.04);c=vec3(1.0,0.60,0.12)*shade+vec3(spec);}
 if(tag>2.5){c=vcolor.rgb;}
 float fog=1.0-exp(-length(Eye-vpos)*0.008);
 gl_FragColor=vec4(mix(c,vec3(0.18,0.24,0.30),fog),1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        math::V,
        viewer::{builder::SceneBuilder, controller::Collider, room::Room},
    };

    /// A floor, a crate standing on it and a wall.
    fn scene() -> Room {
        SceneBuilder::new("lighting")
            .spawn(V(0., 0., 3.), 0.)
            .structural_box("floor", V(0., -0.1, 0.), V(4., 0.1, 4.), V(0.6, 0.5, 0.4))
            .structural_box("crate", V(0., 0.5, 0.), V(0.5, 0.5, 0.5), V(0.8, 0.3, 0.2))
            .structural_box("wall", V(3., 1., 0.), V(0.1, 1., 3.), V(0.7, 0.7, 0.75))
            .build()
            .unwrap()
            .build()
            .unwrap()
    }

    fn digest(meshes: &[Mesh]) -> u64 {
        let mut h = crate::viewer::devkit::StateHasher::new();
        for m in meshes {
            h.u64(m.vertices.len() as u64).u64(m.indices.len() as u64);
            for v in &m.vertices {
                for f in v
                    .position
                    .to_array()
                    .iter()
                    .chain(v.uv.to_array().iter())
                    .chain(v.normal.to_array().iter())
                {
                    h.f32(*f);
                }
                h.bytes(&v.color);
            }
        }
        h.finish()
    }

    /// Vertices on the floor's top face (y = 0, facing up) inside the given x/z window.
    fn floor_top(
        meshes: &[Mesh],
        x: std::ops::Range<f32>,
        z: std::ops::Range<f32>,
    ) -> Vec<[u8; 4]> {
        meshes
            .iter()
            .flat_map(|m| &m.vertices)
            .filter(|v| {
                v.normal.y > 0.99
                    && v.position.y.abs() < 1e-4
                    && x.contains(&v.position.x)
                    && z.contains(&v.position.z)
            })
            .map(|v| v.color)
            .collect()
    }

    fn brightness(colors: &[[u8; 4]]) -> f32 {
        colors
            .iter()
            .map(|c| f32::from(c[0]) + f32::from(c[1]) + f32::from(c[2]))
            .sum::<f32>()
            / colors.len().max(1) as f32
    }

    #[test]
    fn the_house_look_is_what_bake_tagged_produces() {
        let room = scene();
        let tags = [(
            Collider {
                min: V(2., 0., -3.),
                max: V(4., 3., 3.),
            },
            3.,
        )];
        assert_eq!(
            digest(&bake_tagged(&room.world, &tags)),
            digest(&bake_with(&room.world, &Lighting::house(), &tags))
        );
        assert_eq!(
            digest(&bake(&room.world)),
            digest(&bake_with(&room.world, &Lighting::house(), &[]))
        );
        let house = Lighting::house();
        assert_eq!(
            (house.lights.len(), house.shadows, house.ambient_occlusion),
            (3, true, true)
        );
    }

    #[test]
    fn different_lighting_bakes_differently() {
        let room = scene();
        let bake = |l: &Lighting| digest(&bake_with(&room.world, l, &[]));
        let sun = Lighting::sun(V(0.3, 1., 0.2), V(1., 0.95, 0.85), 1.);
        let flat = Lighting::flat(V(0.5, 0.5, 0.5));
        let (a, b, c) = (bake(&Lighting::house()), bake(&sun), bake(&flat));
        assert!(a != b && b != c && a != c);
        assert_eq!(b, bake(&sun), "baking is deterministic");
    }

    #[test]
    fn flat_lighting_is_ambient_only() {
        let room = scene();
        let meshes = bake_with(&room.world, &Lighting::flat(V(0.5, 0.5, 0.5)), &[]);
        // Far from the crate the floor top is colour * ambient * (0.8 + 0.2), gamma corrected.
        let colors = floor_top(&meshes, 1.0..2.5, -2.0..-1.0);
        assert!(!colors.is_empty());
        let expect = |c: f32| ((c * 0.5).powf(1. / 2.2) * 255.).round();
        for c in colors {
            let got = [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])];
            assert!(
                (got[0] - expect(0.6)).abs() <= 1.5
                    && (got[1] - expect(0.5)).abs() <= 1.5
                    && (got[2] - expect(0.4)).abs() <= 1.5,
                "{got:?}"
            );
        }
    }

    #[test]
    fn a_sun_casts_the_crates_shadow_onto_the_floor_and_shadows_can_be_turned_off() {
        let room = scene();
        let sun = Lighting::sun(V(0.3, 1., 0.2), V(1., 1., 1.), 1.);
        let unshadowed = Lighting {
            shadows: false,
            ..sun.clone()
        };
        // The crate is 1 m tall and the sun leans towards +x/+z, so the shadow lands at -x/-z of the crate.
        let shadowed = floor_top(&bake_with(&room.world, &sun, &[]), -0.85..-0.6, -0.3..0.1);
        let open = floor_top(
            &bake_with(&room.world, &unshadowed, &[]),
            -0.85..-0.6,
            -0.3..0.1,
        );
        assert!(!shadowed.is_empty() && shadowed.len() == open.len());
        assert!(
            brightness(&shadowed) < brightness(&open) * 0.8,
            "{} vs {}",
            brightness(&shadowed),
            brightness(&open)
        );
        // Away from the crate the two bakes agree.
        let far_a = floor_top(&bake_with(&room.world, &sun, &[]), 1.0..2.5, 1.0..2.5);
        let far_b = floor_top(
            &bake_with(&room.world, &unshadowed, &[]),
            1.0..2.5,
            1.0..2.5,
        );
        assert!((brightness(&far_a) - brightness(&far_b)).abs() < 8.);
    }
}
