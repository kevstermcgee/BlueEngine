//! Prevalidated static art: the same data works in portable native/browser presentation.
//! Servers read [`ColliderMetadata`] alone; importing/decoding images is authoring-only.
use serde::{Deserialize, Serialize};

/// Matches the engine's default draw capacities. Import tests check the kit constants.
pub const MAX_VERTICES: usize = 9_000;
// Macroquad's standard portable Conf has 5,000 indices; the native kit configures 30,000.
// Stay under both without requiring game authors to discover/configure internal draw limits.
pub const MAX_INDICES: usize = 4_998;
pub const MAX_MODEL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelChunk {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u16>,
    pub texture: Option<usize>,
}

/// One-texel sampler border is baked at import, so UVs in one tile retain glTF filtering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTexture {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
    pub linear: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticModel {
    pub version: u32,
    pub chunks: Vec<ModelChunk>,
    pub textures: Vec<ModelTexture>,
}

/// Precomputed conservative box, separate from art for rendering-free authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColliderMetadata {
    pub version: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
    pub triangle_count: usize,
    pub chunk_count: usize,
    pub collision: String,
}

impl ColliderMetadata {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4096 {
            return Err("collider metadata exceeds 4096 bytes".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if value.version != 1
            || value.collision != "box"
            || value.triangle_count == 0
            || value.chunk_count == 0
        {
            return Err("unsupported/empty static model collider".into());
        }
        for axis in 0..3 {
            let lo = value.bounds_min[axis];
            let hi = value.bounds_max[axis];
            let center = value.center[axis];
            let half = value.half_extents[axis];
            if ![lo, hi, center, half].iter().all(|v| v.is_finite())
                || lo > hi
                || half <= 0.0
                || (center - (lo + hi) * 0.5).abs() > 1e-5
                || half + 1e-5 < (hi - lo) * 0.5
            {
                return Err(format!("invalid/conservative box on axis {axis}"));
            }
        }
        Ok(value)
    }
}

impl StaticModel {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MODEL_BYTES {
            return Err("model exceeds 16 MiB budget; split the asset".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.chunks.is_empty() {
            return Err("unsupported/empty static model".into());
        }
        for texture in &self.textures {
            if texture.width == 0
                || texture.height == 0
                || texture.width > 1026
                || texture.height > 1026
                || texture.rgba.len()
                    != usize::from(texture.width) * usize::from(texture.height) * 4
            {
                return Err("invalid texture dimensions/pixels".into());
            }
        }
        for chunk in &self.chunks {
            if chunk.vertices.is_empty()
                || chunk.vertices.len() > MAX_VERTICES
                || chunk.indices.is_empty()
                || chunk.indices.len() > MAX_INDICES
                || chunk.indices.len() % 3 != 0
                || chunk
                    .indices
                    .iter()
                    .any(|&i| usize::from(i) >= chunk.vertices.len())
                || chunk.texture.is_some_and(|i| i >= self.textures.len())
            {
                return Err("invalid/oversized triangle chunk; reimport the asset".into());
            }
            for v in &chunk.vertices {
                if v.position
                    .iter()
                    .chain(&v.normal)
                    .chain(&v.uv)
                    .chain(&v.color)
                    .any(|x| !x.is_finite())
                    || v.color.iter().any(|x| !(0.0..=1.0).contains(x))
                    || v.uv.iter().any(|x| !(0.0..=1.0).contains(x))
                {
                    return Err("invalid static model vertex".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(any(feature = "presentation", feature = "two-d"))]
pub mod draw {
    use super::*;
    use macroquad::prelude::*;
    pub use macroquad::prelude::{Mat4, Vec3};

    /// Upload once, then instantiate meshes. Requires an active graphics context.
    pub struct Model {
        data: StaticModel,
        textures: Vec<Texture2D>,
    }

    impl Model {
        pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
            let data = StaticModel::from_json(bytes)?;
            let textures = data
                .textures
                .iter()
                .map(|t| {
                    let texture = Texture2D::from_rgba8(t.width, t.height, &t.rgba);
                    texture.set_filter(if t.linear {
                        FilterMode::Linear
                    } else {
                        FilterMode::Nearest
                    });
                    texture
                })
                .collect();
            Ok(Self { data, textures })
        }

        /// Real UVs and normals, suitable for World::mesh or a custom material/shader.
        pub fn meshes(&self, transform: Mat4) -> Result<Vec<Mesh>, String> {
            let determinant = transform.determinant();
            if !transform.is_finite()
                || !determinant.is_finite()
                || determinant.abs() < 1e-12
                || transform.w_axis.w != 1.0
                || transform.x_axis.w != 0.0
                || transform.y_axis.w != 0.0
                || transform.z_axis.w != 0.0
            {
                return Err("model transform must be finite, affine and invertible".into());
            }
            let normal_transform = transform.inverse().transpose();
            Ok(self
                .data
                .chunks
                .iter()
                .map(|chunk| {
                    let vertices = chunk
                        .vertices
                        .iter()
                        .map(|v| {
                            let p = transform.transform_point3(Vec3::from_array(v.position));
                            let n = normal_transform
                                .transform_vector3(Vec3::from_array(v.normal))
                                .normalize_or_zero();
                            let mut vertex = Vertex::new(
                                p.x,
                                p.y,
                                p.z,
                                v.uv[0],
                                v.uv[1],
                                Color::new(v.color[0], v.color[1], v.color[2], v.color[3]),
                            );
                            vertex.normal = n.extend(0.0);
                            vertex
                        })
                        .collect();
                    let mut indices = chunk.indices.clone();
                    if determinant < 0.0 {
                        for tri in indices.chunks_exact_mut(3) {
                            tri.swap(1, 2);
                        }
                    }
                    Mesh {
                        vertices,
                        indices,
                        texture: chunk.texture.map(|i| self.textures[i].clone()),
                    }
                })
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collider_rejects_underreported_or_nonfinite_bounds() {
        let mut c = ColliderMetadata {
            version: 1,
            bounds_min: [-1.0, 0.0, -1.0],
            bounds_max: [1.0, 2.0, 1.0],
            center: [0.0, 1.0, 0.0],
            half_extents: [1.0; 3],
            triangle_count: 12,
            chunk_count: 1,
            collision: "box".into(),
        };
        assert!(ColliderMetadata::from_json(&serde_json::to_vec(&c).unwrap()).is_ok());
        c.half_extents[0] = 0.9;
        assert!(ColliderMetadata::from_json(&serde_json::to_vec(&c).unwrap()).is_err());
    }
    #[test]
    fn invalid_art_does_not_reach_graphics_upload() {
        let mut m = StaticModel {
            version: 1,
            chunks: vec![ModelChunk {
                vertices: vec![ModelVertex {
                    position: [0.0; 3],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0; 2],
                    color: [1.0; 4],
                }],
                indices: vec![0, 0, 2],
                texture: None,
            }],
            textures: vec![],
        };
        assert!(m.validate().is_err());
        m.chunks[0].indices[2] = 0;
        m.chunks[0].texture = Some(0);
        assert!(m.validate().is_err());
    }
}
