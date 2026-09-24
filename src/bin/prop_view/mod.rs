//! Reuses each prop mesh; rigid transforms update vertices without re-baking the map.
use macroquad::prelude::*;
use vesper3d::math::V;
use vesper3d::viewer::{mesh, prop_physics::PropPhysics};
pub struct Props {
    meshes: Vec<Vec<Mesh>>,
    original: Vec<Vec<Vec<Vertex>>>,
}
impl Props {
    pub fn new(physics: &PropPhysics) -> Self {
        let meshes: Vec<Vec<Mesh>> = physics
            .props
            .iter()
            .map(|p| {
                mesh::bake_tagged(
                    &p.local_world,
                    &[(
                        vesper3d::viewer::controller::Collider {
                            min: V::ONE * -10000.,
                            max: V::ONE * 10000.,
                        },
                        3.,
                    )],
                )
            })
            .collect();
        let original = meshes
            .iter()
            .map(|ms| ms.iter().map(|m| m.vertices.clone()).collect())
            .collect();
        Self { meshes, original }
    }
    pub fn draw(&mut self, physics: &PropPhysics) {
        for ((meshes, original), p) in self
            .meshes
            .iter_mut()
            .zip(&self.original)
            .zip(&physics.props)
        {
            for (m, vertices) in meshes.iter_mut().zip(original) {
                for (v, base) in m.vertices.iter_mut().zip(vertices) {
                    v.position = mesh::vec(
                        p.transform
                            .point(V(base.position.x, base.position.y, base.position.z) - p.origin),
                    );
                    let n = p
                        .transform
                        .vector(V(base.normal.x, base.normal.y, base.normal.z));
                    v.normal = vec4(n.0, n.1, n.2, base.normal.w);
                }
                draw_mesh(m);
            }
        }
    }
}
