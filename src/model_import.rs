//! Bounded, offline glTF authoring. No importer/image dependency enters game or server builds.
use crate::asset_model::*;
use crate::viewer::kit::{lint, Template, Vert};
use base64::Engine;
use macroquad::prelude::*;
use std::{collections::HashMap, io::Cursor, path::Path};

const MAX_INPUT: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct ImportedModel {
    pub model: StaticModel,
    pub collider: ColliderMetadata,
    pub warnings: Vec<String>,
}

fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_INPUT {
        return Err("input exceeds 32 MiB; split the asset".into());
    }
    Ok(bytes)
}

fn resource(root: &Path, uri: &str) -> Result<Vec<u8>, String> {
    if uri.starts_with("data:") {
        let (kind, encoded) = uri.split_once(',').ok_or("invalid data URI")?;
        if !kind.ends_with(";base64") || encoded.len() > MAX_INPUT * 4 / 3 + 4 {
            return Err("invalid/oversized base64 URI".into());
        }
        return base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string());
    }
    // Never fetch remote resources or walk out of the source directory, including through symlinks.
    if uri.contains([':', '?', '#', '%', '\\']) {
        return Err(
            "resource URI must be a local unescaped relative path (or base64 data URI)".into(),
        );
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root
        .join(uri)
        .canonicalize()
        .map_err(|e| format!("resource {uri}: {e}"))?;
    if !path.starts_with(&root) || Path::new(uri).is_absolute() {
        return Err("resource escapes model directory".into());
    }
    bounded_read(&path)
}

/// All scenes are static. Scale is explicit, then the chosen scene is normalized to bottom-center.
pub fn import(path: &Path, scale: f32) -> Result<ImportedModel, String> {
    import_with_options(path, scale, false)
}

/// Optional, explicit repair removes only triangles reported as zero-area, then reruns the full lint.
pub fn import_with_options(
    path: &Path,
    scale: f32,
    repair_degenerate: bool,
) -> Result<ImportedModel, String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err("scale must be positive and finite".into());
    }
    let bytes = bounded_read(path)?;
    let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| format!("invalid glTF: {e}"))?;
    if gltf.skins().next().is_some() || gltf.animations().next().is_some() {
        return Err("skeletal/animated assets are outside static import; export a static pose or use a custom presentation adapter".into());
    }
    if gltf
        .extensions_used()
        .any(|extension| extension != "KHR_materials_unlit")
    {
        return Err("glTF extensions are not supported by this base-color importer; bake them or use a custom presentation adapter".into());
    }
    let root = path.parent().unwrap_or(Path::new("."));
    let mut total_bytes = bytes.len();
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("missing GLB binary buffer")?,
            gltf::buffer::Source::Uri(uri) => resource(root, uri)?,
        };
        total_bytes = total_bytes
            .checked_add(data.len())
            .ok_or("resource size overflow")?;
        if total_bytes > MAX_INPUT || data.len() < buffer.length() {
            return Err("buffers exceed 32 MiB aggregate or are truncated".into());
        }
        buffers.push(data);
    }
    for view in gltf.views() {
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > buffers[view.buffer().index()].len())
        {
            return Err("buffer view exceeds actual resource".into());
        }
    }
    for accessor in gltf.accessors() {
        if accessor.sparse().is_some() {
            return Err("sparse accessors: export dense static vertex data".into());
        }
        let view = accessor.view().ok_or("accessor has no buffer view")?;
        let stride = view.stride().unwrap_or(accessor.size());
        let end = accessor
            .count()
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(accessor.offset()))
            .and_then(|n| n.checked_add(accessor.size()));
        if accessor.count() == 0
            || stride < accessor.size()
            || end.is_none_or(|end| end > view.length())
        {
            return Err("accessor exceeds its buffer view or has an invalid stride".into());
        }
    }
    let mut textures = Vec::new();
    for texture in gltf.textures() {
        let data = match texture.source().source() {
            gltf::image::Source::View { view, .. } => buffers[view.buffer().index()]
                [view.offset()..view.offset() + view.length()]
                .to_vec(),
            gltf::image::Source::Uri { uri, .. } => resource(root, uri)?,
        };
        total_bytes = total_bytes
            .checked_add(data.len())
            .ok_or("image size overflow")?;
        if total_bytes > MAX_INPUT {
            return Err("images exceed 32 MiB aggregate input budget".into());
        }
        let mut reader = image::io::Reader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        if !matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ) {
            return Err("base-color images must be PNG or JPEG".into());
        }
        let mut limits = image::io::Limits::default();
        limits.max_image_width = Some(1024);
        limits.max_image_height = Some(1024);
        limits.max_alloc = Some(16 * 1024 * 1024);
        reader.limits(limits);
        let mut image = reader
            .decode()
            .map_err(|e| format!("image decode (1024px budget): {e}"))?
            .to_rgba8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err("empty texture".into());
        }
        // glTF OPAQUE ignores factor/vertex/texture alpha. MASK/BLEND are rejected below.
        for pixel in image.pixels_mut() {
            pixel[3] = 255;
        }
        let sampler = texture.sampler();
        let coordinate = |n: u32, size: u32, wrap: gltf::texture::WrappingMode| -> u32 {
            if n > 0 && n <= size {
                return n - 1;
            }
            match wrap {
                gltf::texture::WrappingMode::Repeat => {
                    if n == 0 {
                        size - 1
                    } else {
                        0
                    }
                }
                _ => {
                    if n == 0 {
                        0
                    } else {
                        size - 1
                    }
                }
            }
        };
        let mut rgba = Vec::with_capacity((width + 2) as usize * (height + 2) as usize * 4);
        for y in 0..height + 2 {
            for x in 0..width + 2 {
                rgba.extend_from_slice(
                    &image
                        .get_pixel(
                            coordinate(x, width, sampler.wrap_s()),
                            coordinate(y, height, sampler.wrap_t()),
                        )
                        .0,
                );
            }
        }
        let linear = !matches!(
            sampler.mag_filter(),
            Some(gltf::texture::MagFilter::Nearest)
        );
        if sampler.min_filter().is_some_and(|m| {
            !matches!(
                m,
                gltf::texture::MinFilter::Linear | gltf::texture::MinFilter::Nearest
            )
        }) {
            return Err("mipmapped texture sampler: export a non-mipmapped base-color sampler or use a custom material".into());
        }
        if sampler
            .min_filter()
            .is_some_and(|m| matches!(m, gltf::texture::MinFilter::Linear) != linear)
        {
            return Err("different min/mag filters require a custom material".into());
        }
        textures.push(ModelTexture {
            width: (width + 2) as u16,
            height: (height + 2) as u16,
            rgba,
            linear,
        });
    }
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or("no glTF scene")?;
    let mut primitives = Vec::new();
    let mut warnings = Vec::new();
    for node in scene.nodes() {
        collect(
            node,
            Mat4::IDENTITY,
            &buffers,
            &textures,
            &mut primitives,
            &mut warnings,
            0,
        )?;
    }
    if primitives.is_empty() {
        return Err("selected glTF scene contains no static triangles".into());
    }
    let total_vertices: usize = primitives.iter().map(|p| p.vertices.len()).sum();
    if total_vertices > u16::MAX as usize {
        return Err(format!("{total_vertices} vertices exceeds 65535 full-lint budget; split the model and import each part (nothing imported)"));
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for primitive in &primitives {
        for vertex in &primitive.vertices {
            let p = Vec3::from_array(vertex.position) * scale;
            min = min.min(p);
            max = max.max(p);
        }
    }
    let origin = vec3((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
    let mut template = Template::new();
    for primitive in &mut primitives {
        let offset = template.verts.len() as u16;
        for v in &mut primitive.vertices {
            v.position = (Vec3::from_array(v.position) * scale - origin).to_array();
            if v.position.iter().any(|x| !x.is_finite()) {
                return Err("scale overflowed model coordinates; use a smaller scale".into());
            }
            template.verts.push(Vert {
                p: Vec3::from_array(v.position),
                n: Vec3::from_array(v.normal),
                c: [v.color[0], v.color[1], v.color[2]],
                e: 0.0,
                a: v.color[3],
            });
        }
        template
            .idx
            .extend(primitive.indices.iter().map(|&i| i + offset));
    }
    let mut findings = lint::lint(&template);
    if repair_degenerate
        && !findings.is_empty()
        && findings
            .iter()
            .all(|d| matches!(d, lint::Defect::ZeroArea { .. }))
    {
        let removed: std::collections::HashSet<_> = findings
            .iter()
            .filter_map(|d| match d {
                lint::Defect::ZeroArea { tri } => Some(*tri),
                _ => None,
            })
            .collect();
        let original_count = template.idx.len() / 3;
        let mut offset = 0;
        for primitive in &mut primitives {
            let count = primitive.indices.len() / 3;
            primitive.indices = primitive
                .indices
                .chunks_exact(3)
                .enumerate()
                .filter(|(i, _)| !removed.contains(&(offset + i)))
                .flat_map(|(_, t)| t.iter().copied())
                .collect();
            offset += count;
        }
        template.idx = template
            .idx
            .chunks_exact(3)
            .enumerate()
            .filter(|(i, _)| !removed.contains(i))
            .flat_map(|(_, t)| t.iter().copied())
            .collect();
        let mut removed_ids: Vec<_> = removed.iter().copied().collect();
        removed_ids.sort_unstable();
        warnings.push(format!("Explicit zero-area repair: removed {} of {original_count} triangles; full kit::lint rerun. Original triangle IDs: {removed_ids:?}",removed.len()));
        findings = lint::lint(&template);
    }
    if !findings.is_empty() {
        return Err(format!(
            "kit::lint rejected import (no output): {}",
            findings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let triangle_count = template.idx.len() / 3;
    if triangle_count == 0 {
        return Err("repair left no triangles; no import written".into());
    }
    let chunks: Vec<_> = primitives.iter().flat_map(split).collect();
    let collider = ColliderMetadata {
        version: 1,
        bounds_min: (min - origin).to_array(),
        bounds_max: (max - origin).to_array(),
        center: ((min + max) * 0.5 - origin).to_array(),
        half_extents: ((max - min) * 0.5).max(Vec3::splat(0.001)).to_array(),
        triangle_count,
        chunk_count: chunks.len(),
        collision: "box".into(),
    };
    let model = StaticModel {
        version: 1,
        chunks,
        textures,
    };
    model.validate()?;
    ColliderMetadata::from_json(&serde_json::to_vec(&collider).map_err(|e| e.to_string())?)?;
    if serde_json::to_vec(&model).map_err(|e| e.to_string())?.len() > MAX_MODEL_BYTES {
        return Err("processed model exceeds 16 MiB; split the asset".into());
    }
    warnings.sort();
    warnings.dedup();
    Ok(ImportedModel {
        model,
        collider,
        warnings,
    })
}

fn collect(
    node: gltf::Node<'_>,
    parent: Mat4,
    buffers: &[Vec<u8>],
    textures: &[ModelTexture],
    out: &mut Vec<ModelChunk>,
    warnings: &mut Vec<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("cyclic/excessively deep scene graph".into());
    }
    let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    let det = transform.determinant();
    if !transform.is_finite()
        || !det.is_finite()
        || det.abs() < 1e-12
        || transform.w_axis.w != 1.0
        || transform.x_axis.w != 0.0
        || transform.y_axis.w != 0.0
        || transform.z_axis.w != 0.0
    {
        return Err("singular/nonfinite node transform".into());
    }
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            if primitive.morph_targets().next().is_some() {
                return Err("morph targets: export a static pose".into());
            }
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(
                    "only triangle-list static primitives supported; triangulate before import"
                        .into(),
                );
            }
            let material = primitive.material();
            if material.alpha_mode() != gltf::material::AlphaMode::Opaque {
                return Err("alpha modes require a custom presentation adapter; this importer handles opaque base color".into());
            }
            if material.double_sided() {
                return Err(
                    "double-sided material: export explicit back faces or use a custom material"
                        .into(),
                );
            }
            if material.normal_texture().is_some()
                || material.occlusion_texture().is_some()
                || material.emissive_texture().is_some()
                || material.emissive_factor() != [0.0; 3]
                || material
                    .pbr_metallic_roughness()
                    .metallic_roughness_texture()
                    .is_some()
            {
                warnings.push(
                    "Only base color is imported; PBR/normal/emissive maps need a custom material."
                        .into(),
                );
            }
            let factor = material.pbr_metallic_roughness().base_color_factor();
            let texture = material.pbr_metallic_roughness().base_color_texture();
            if texture.as_ref().is_some_and(|t| t.tex_coord() != 0) {
                return Err("base-color texture requires TEXCOORD_0; export that UV set".into());
            }
            let reader = primitive.reader(|b| Some(buffers[b.index()].as_slice()));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("mesh missing POSITION")?
                .collect();
            if positions.len() > u16::MAX as usize
                || out.iter().map(|c| c.vertices.len()).sum::<usize>() + positions.len()
                    > u16::MAX as usize
            {
                return Err("65535 full-lint vertex budget exceeded; split the model".into());
            }
            let normals: Option<Vec<_>> = reader.read_normals().map(Iterator::collect);
            let uvs: Option<Vec<_>> = reader.read_tex_coords(0).map(|x| x.into_f32().collect());
            let colors: Option<Vec<_>> = reader.read_colors(0).map(|x| x.into_rgba_f32().collect());
            if normals.as_ref().is_some_and(|v| v.len() != positions.len())
                || uvs.as_ref().is_some_and(|v| v.len() != positions.len())
                || colors.as_ref().is_some_and(|v| v.len() != positions.len())
                || texture.is_some() && uvs.is_none()
            {
                return Err("mismatched/missing vertex attributes".into());
            }
            let mut indices: Vec<u32> = reader
                .read_indices()
                .map(|x| x.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if indices.is_empty()
                || !indices.len().is_multiple_of(3)
                || indices.iter().any(|&i| i as usize >= positions.len())
            {
                return Err("invalid triangle indices".into());
            }
            if det < 0.0 {
                for tri in indices.chunks_exact_mut(3) {
                    tri.swap(1, 2);
                }
            }
            let transformed: Vec<_> = positions
                .iter()
                .map(|&p| transform.transform_point3(Vec3::from_array(p)))
                .collect();
            let generated = if normals.is_none() {
                let mut generated = vec![Vec3::ZERO; positions.len()];
                for tri in indices.chunks_exact(3) {
                    let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    let n =
                        (transformed[b] - transformed[a]).cross(transformed[c] - transformed[a]);
                    for i in [a, b, c] {
                        generated[i] += n;
                    }
                }
                Some(generated)
            } else {
                None
            };
            let normal_transform = transform.inverse().transpose();
            let mut vertices = Vec::new();
            for (i, p) in transformed.iter().enumerate() {
                let n = normals
                    .as_ref()
                    .map(|ns| normal_transform.transform_vector3(Vec3::from_array(ns[i])))
                    .unwrap_or_else(|| generated.as_ref().unwrap()[i])
                    .normalize_or_zero();
                if !p.is_finite() || !n.is_finite() || n.length_squared() < 0.9 {
                    return Err("nonfinite vertex or missing/zero normal".into());
                }
                let mut uv = uvs.as_ref().map(|v| v[i]).unwrap_or([0.0; 2]);
                if let Some(info) = &texture {
                    if uv
                        .iter()
                        .any(|&v| !v.is_finite() || !(0.0..=1.0).contains(&v))
                    {
                        return Err("UVs outside one tile need baking or a custom repeat shader; not silently clamped".into());
                    }
                    let t = &textures[info.texture().index()];
                    uv = [
                        (uv[0] * f32::from(t.width - 2) + 1.0) / f32::from(t.width),
                        (uv[1] * f32::from(t.height - 2) + 1.0) / f32::from(t.height),
                    ];
                } else {
                    uv = [0.0; 2];
                }
                let mut color = colors.as_ref().map(|v| v[i]).unwrap_or([1.0; 4]);
                for channel in 0..4 {
                    color[channel] *= factor[channel];
                }
                color[3] = 1.0;
                vertices.push(ModelVertex {
                    position: p.to_array(),
                    normal: n.to_array(),
                    uv,
                    color,
                });
            }
            out.push(ModelChunk {
                vertices,
                indices: indices.into_iter().map(|i| i as u16).collect(),
                texture: texture.map(|t| t.texture().index()),
            });
        }
    }
    for child in node.children() {
        collect(
            child,
            transform,
            buffers,
            textures,
            out,
            warnings,
            depth + 1,
        )?;
    }
    Ok(())
}

fn split(primitive: &ModelChunk) -> Vec<ModelChunk> {
    let mut chunks = Vec::new();
    let mut chunk = ModelChunk {
        vertices: vec![],
        indices: vec![],
        texture: primitive.texture,
    };
    let mut remap = HashMap::new();
    for triangle in primitive.indices.chunks_exact(3) {
        let needed = triangle.iter().filter(|i| !remap.contains_key(*i)).count();
        if chunk.vertices.len() + needed > MAX_VERTICES || chunk.indices.len() + 3 > MAX_INDICES {
            chunks.push(chunk);
            chunk = ModelChunk {
                vertices: vec![],
                indices: vec![],
                texture: primitive.texture,
            };
            remap.clear();
        }
        for &index in triangle {
            let mapped = *remap.entry(index).or_insert_with(|| {
                let next = chunk.vertices.len() as u16;
                chunk
                    .vertices
                    .push(primitive.vertices[index as usize].clone());
                next
            });
            chunk.indices.push(mapped);
        }
    }
    if !chunk.indices.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn with_source(value: serde_json::Value, test: impl FnOnce(&Path)) {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "be2-import-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("model.gltf");
        std::fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
        test(&source);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn triangle_document(count: usize) -> serde_json::Value {
        let mut positions = Vec::new();
        for i in 0..count {
            let x = (i % 100) as f32 * 2.0;
            let y = (i / 100) as f32 * 2.0;
            positions.extend([x, y, 0.0, x + 1.0, y, 0.0, x, y + 1.0, 0.0]);
        }
        let data: Vec<u8> = positions.iter().flat_map(|p| p.to_le_bytes()).collect();
        json!({"asset":{"version":"2.0"},"buffers":[{"uri":format!("data:application/octet-stream;base64,{}", base64::engine::general_purpose::STANDARD.encode(&data)),"byteLength":data.len()}],"bufferViews":[{"buffer":0,"byteLength":data.len()}],"accessors":[{"bufferView":0,"componentType":5126,"count":count*3,"type":"VEC3","min":[0,0,0],"max":[200,200,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0})
    }

    #[test]
    fn cc0_furniture_passes_real_geometry_lint_and_bounds() {
        for name in ["chair", "table", "lampSquareFloor"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/models/{name}.glb"));
            let result = import_with_options(&path, 2.5, true).unwrap();
            assert!(result.collider.triangle_count > 20);
            assert!(result.collider.bounds_min[1].abs() < 1e-6);
            assert!(result.collider.half_extents.iter().all(|&v| v > 0.0));
            assert_eq!(
                result.collider.triangle_count,
                result
                    .model
                    .chunks
                    .iter()
                    .map(|c| c.indices.len() / 3)
                    .sum::<usize>()
            );
        }
    }

    #[test]
    fn committed_art_and_colliders_equal_current_conversion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let pack_bytes: usize = std::fs::read_dir(root.join("tests/fixtures/models"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len() as usize)
            .sum();
        assert!(
            pack_bytes < 256 * 1024,
            "fixtures must stay small; use external packs"
        );
        for (source, id, scale, repair) in [
            ("chair.glb", "chair", 2.5, true),
            ("table.glb", "table", 2.5, true),
            ("lampSquareFloor.glb", "floor-lamp", 2.5, false),
            ("textured-cube.gltf", "textured-cube", 1.0, false),
        ] {
            let result = import_with_options(
                &root.join("tests/fixtures/models").join(source),
                scale,
                repair,
            )
            .unwrap();
            let dir = root.join("assets/models/cc0").join(id);
            let committed_model =
                StaticModel::from_json(&std::fs::read(dir.join("model.json")).unwrap()).unwrap();
            let committed_collider =
                ColliderMetadata::from_json(&std::fs::read(dir.join("collider.json")).unwrap())
                    .unwrap();
            assert!(
                committed_model == result.model,
                "reimport fixture {id}/model.json"
            );
            assert!(
                committed_collider == result.collider,
                "reimport fixture {id}/collider.json"
            );
        }
    }

    #[test]
    fn large_import_splits_loudly_without_losing_geometry() {
        with_source(triangle_document(3500), |p| {
            let result = import(p, 1.0).unwrap();
            assert_eq!(result.collider.triangle_count, 3500);
            assert_eq!(result.model.chunks.len(), 3);
        });
        with_source(triangle_document(22000), |p| {
            assert!(import(p, 1.0).unwrap_err().contains("65535"))
        });
    }

    #[test]
    fn malformed_accessors_and_geometry_fail_without_partial_success() {
        let mut value = triangle_document(1);
        value["accessors"][0]["count"] = json!(6);
        with_source(value, |p| {
            assert!(import(p, 1.0).unwrap_err().contains("accessor"))
        });
        let mut value = triangle_document(1);
        value["nodes"][0]["scale"] = json!([0.0, 1.0, 1.0]);
        with_source(value, |p| {
            assert!(import(p, 1.0).unwrap_err().contains("singular"))
        });
        let mut value = triangle_document(1);
        value["buffers"][0]["uri"] = json!("../outside.bin");
        with_source(value, |p| assert!(import(p, 1.0).is_err()));
        let mut value = triangle_document(1);
        value["nodes"][0]["matrix"] = json!([1, 0, 0, 0.5, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
        with_source(value, |p| assert!(import(p, 1.0).is_err()));
    }

    #[test]
    fn base_factor_texture_uvs_and_mirrored_transform_survive_import() {
        let mut value = triangle_document(1);
        let raw_uv: Vec<u8> = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        value["buffers"].as_array_mut().unwrap().push(json!({"uri":format!("data:application/octet-stream;base64,{}",base64::engine::general_purpose::STANDARD.encode(&raw_uv)),"byteLength":raw_uv.len()}));
        value["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":1,"byteLength":raw_uv.len()}));
        value["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
        value["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
        value["meshes"][0]["primitives"][0]["material"] = json!(0);
        let image = image::RgbaImage::from_fn(2, 2, |x, y| {
            image::Rgba([x as u8 * 200, y as u8 * 200, 100, 64])
        });
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageOutputFormat::Png)
            .unwrap();
        value["images"] = json!([{"uri":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(png.into_inner()))}]);
        value["textures"] = json!([{"source":0}]);
        value["materials"] = json!([{"pbrMetallicRoughness":{"baseColorFactor":[0.5,0.25,1.0,0.25],"baseColorTexture":{"index":0}}}]);
        value["nodes"][0]["scale"] = json!([-2.0, 3.0, 1.0]);
        with_source(value.clone(), |p| {
            let result = import(p, 1.0).unwrap();
            let t = &result.model.textures[0];
            assert_eq!((t.width, t.height), (4, 4));
            assert_eq!(&t.rgba[..4], &[200, 200, 100, 255]); // Default repeat corner border.
            let c = &result.model.chunks[0];
            assert_eq!(c.texture, Some(0));
            assert!(c
                .vertices
                .iter()
                .all(|v| v.color == [0.5, 0.25, 1.0, 1.0] && v.normal[2] > 0.99));
            assert_eq!(c.vertices[0].uv, [0.25, 0.25]);
            assert_eq!(
                result.collider.bounds_max[0] - result.collider.bounds_min[0],
                2.0
            );
        });
        value["materials"][0]["alphaMode"] = json!("BLEND");
        with_source(value, |p| {
            assert!(import(p, 1.0).unwrap_err().contains("alpha"))
        });
    }
    #[test]
    fn splitting_keeps_every_triangle_and_draw_limit_contract() {
        assert_eq!(MAX_VERTICES, crate::viewer::kit::MAX_MESH_VERTICES);
        const { assert!(MAX_INDICES <= crate::viewer::kit::MAX_MESH_INDICES) };
        let default = macroquad::conf::Conf::default();
        assert!(MAX_VERTICES < default.draw_call_vertex_capacity);
        assert!(MAX_INDICES < default.draw_call_index_capacity);
        let p = ModelChunk {
            vertices: (0..12000)
                .map(|i| ModelVertex {
                    position: [i as f32, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0; 2],
                    color: [1.0; 4],
                })
                .collect(),
            indices: (0..12000).collect(),
            texture: None,
        };
        let chunks = split(&p);
        assert_eq!(chunks.len(), 3);
        let restored: Vec<_> = chunks
            .iter()
            .flat_map(|c| {
                c.indices
                    .iter()
                    .map(|&i| c.vertices[i as usize].position[0] as usize)
            })
            .collect();
        assert_eq!(restored, (0..12000).collect::<Vec<_>>());
        assert!(chunks
            .iter()
            .all(|c| c.vertices.len() <= MAX_VERTICES && c.indices.len() <= MAX_INDICES));
    }
}
