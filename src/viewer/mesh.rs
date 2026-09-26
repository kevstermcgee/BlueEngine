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
pub fn bake(world: &World) -> Vec<Mesh> {
    bake_tagged(world, &[])
}
pub fn bake_tagged(world: &World, tags: &[(super::controller::Collider, f32)]) -> Vec<Mesh> {
    bake_tagged_entities(world, tags, &[]).0
}

/// Bake selected semantic boxes separately so a client can change their presentation
/// without rebuilding the static world. Entity order in the result matches `entities`.
pub fn bake_tagged_entities(
    world: &World,
    tags: &[(super::controller::Collider, f32)],
    entity_bounds: &[super::controller::Collider],
) -> (Vec<Mesh>, Vec<Vec<Mesh>>) {
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
                        let light = 0.55 + 0.45 * n.dot(V(-0.4, 0.8, -0.3).norm()).max(0.);
                        let c = instance.material.color * light;
                        V(c.0.sqrt(), c.1.sqrt(), c.2.sqrt())
                    } else {
                        shade(world, instance, p, n)
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

fn shade(world: &World, instance: &Instance, p: V, n: V) -> V {
    let mat = &instance.material;
    if mat.emission > 0. {
        return (mat.color * (0.75 + mat.emission * 0.25)).min(V::ONE);
    }
    let origin = p + n * 0.012;
    let mut light = V(0.24, 0.27, 0.31) * (0.8 + 0.2 * n.1);
    // Actual Vesper intersections provide static contact shadows.
    for (pos, color, power) in [
        (V(-4.8, 3., 1.0), V(0.72, 0.87, 1.), 18.),
        (V(2.4, 3.25, -2.5), V(1., 0.86, 0.66), 15.),
        (V(2.4, 3.25, 3.), V(0.82, 0.91, 1.), 10.),
    ] {
        for offset in [
            V(-0.18, 0., -0.18),
            V(0.18, 0., -0.18),
            V(-0.18, 0., 0.18),
            V(0.18, 0., 0.18),
        ] {
            let delta = pos + offset - origin;
            let distance = delta.length();
            let direction = delta / distance;
            let ndl = n.dot(direction).max(0.);
            if ndl > 0.
                && world
                    .hit(
                        Ray {
                            o: origin,
                            d: direction,
                        },
                        distance - 0.025,
                        true,
                    )
                    .is_none()
            {
                light = light + color * (ndl * power * 0.25 / (3. + distance * distance));
            }
        }
    }

    let tangent = if n.1.abs() < 0.9 {
        n.cross(V(0., 1., 0.)).norm()
    } else {
        n.cross(V(1., 0., 0.)).norm()
    };
    let bitangent = n.cross(tangent);
    let mut ao = 1.;
    for d in [n, (n + tangent * 0.7).norm(), (n + bitangent * 0.7).norm()] {
        if let Some(h) = world.hit(Ray { o: origin, d }, 0.7, false) {
            ao -= 0.14 * (1. - h.t / 0.7);
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
