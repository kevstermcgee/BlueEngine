//! One planar reflection pass with an off-axis aperture and an explicit depth buffer.
//! Draw the reflected world (excluding mirror surfaces), then the main view, then `draw_surface`.
//! This does not recurse, reflect sky directions, or implement rough/PBR reflection.
use macroquad::{camera::Camera, prelude::*};

/// Validated rectangular aperture. `right × up` points towards the viewer side.
#[derive(Clone, Copy, Debug)]
pub struct MirrorPlane {
    center: Vec3,
    right: Vec3,
    up: Vec3,
    normal: Vec3,
    half: Vec2,
}
impl MirrorPlane {
    /// Axes must be finite, nonzero and perpendicular; they are normalized here.
    pub fn new(center: Vec3, right: Vec3, up: Vec3, size: Vec2) -> Result<Self, &'static str> {
        if !center.is_finite()
            || !right.is_finite()
            || !up.is_finite()
            || !size.is_finite()
            || right.length_squared() < 1e-8
            || up.length_squared() < 1e-8
            || size.x <= 0.
            || size.y <= 0.
        {
            return Err("mirror needs finite geometry and positive dimensions");
        }
        let (right, up) = (right.normalize(), up.normalize());
        if right.dot(up).abs() > 1e-4 {
            return Err("mirror axes must be perpendicular");
        }
        Ok(Self {
            center,
            right,
            up,
            normal: right.cross(up),
            half: size * 0.5,
        })
    }
    /// Returns no pass behind or within 2 cm of the plane, or beyond `far`.
    /// Near clipping is at the mirror plane, excluding geometry behind its reflective side.
    pub fn projection(&self, eye: Vec3, far: f32) -> Option<(Vec3, Mat4)> {
        if !eye.is_finite() || !far.is_finite() {
            return None;
        }
        let rel = eye - self.center;
        let distance = rel.dot(self.normal);
        if distance <= 0.02 || far <= distance + 0.01 {
            return None;
        }
        let reflected = eye - self.normal * (2. * distance);
        let x = rel.dot(self.right);
        let y = rel.dot(self.up);
        let near = distance;
        let (l, r, b, t) = (
            x - self.half.x,
            x + self.half.x,
            -y - self.half.y,
            -y + self.half.y,
        );
        let projection = Mat4::from_cols(
            vec4(2. * near / (r - l), 0., 0., 0.),
            vec4(0., 2. * near / (t - b), 0., 0.),
            vec4(
                (r + l) / (r - l),
                (t + b) / (t - b),
                -(far + near) / (far - near),
                -1.,
            ),
            vec4(0., 0., -2. * far * near / (far - near), 0.),
        );
        Some((
            reflected,
            projection * Mat4::look_at_rh(reflected, reflected + self.normal, self.up),
        ))
    }
    fn corners(&self) -> [Vec3; 4] {
        let c = self.center + self.normal * 0.001;
        let r = self.right * self.half.x;
        let u = self.up * self.half.y;
        [c - r - u, c + r - u, c + r + u, c - r + u]
    }
}
/// Owns its target, depth attachment, surface UVs and camera adapter. Create after GL initialization.
pub struct PlanarMirror {
    plane: MirrorPlane,
    target: RenderTarget,
    surface: Mesh,
}
impl PlanarMirror {
    /// Positive resolution no larger than 2048 on either axis (bounded extra render cost).
    pub fn new(plane: MirrorPlane, resolution: (u32, u32)) -> Result<Self, &'static str> {
        if resolution.0 == 0 || resolution.1 == 0 || resolution.0 > 2048 || resolution.1 > 2048 {
            return Err("mirror resolution must be 1..=2048 per axis");
        }
        let target = render_target_ex(
            resolution.0,
            resolution.1,
            RenderTargetParams {
                depth: true,
                ..Default::default()
            },
        );
        target.texture.set_filter(FilterMode::Linear);
        let uv = [vec2(1., 0.), vec2(0., 0.), vec2(0., 1.), vec2(1., 1.)];
        let vertices = plane
            .corners()
            .into_iter()
            .zip(uv)
            .map(|(p, uv)| Vertex::new(p.x, p.y, p.z, uv.x, uv.y, WHITE))
            .collect();
        let surface = Mesh {
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(target.texture.clone()),
        };
        Ok(Self {
            plane,
            target,
            surface,
        })
    }
    /// `set_camera(&camera)`, clear and render the world; restore the main camera before drawing surface.
    pub fn camera(&self, eye: Vec3, far: f32) -> Option<MirrorCamera> {
        let (eye, matrix) = self.plane.projection(eye, far)?;
        Some(MirrorCamera {
            eye,
            matrix,
            pass: self.target.render_pass.clone(),
        })
    }
    /// Draw with the default textured material and the main camera. Omit when `camera` returned None.
    pub fn draw_surface(&self) {
        gl_use_default_material();
        draw_mesh(&self.surface);
    }
}
/// Camera for a reflection pass; its eye also supplies lighting/fog uniforms.
pub struct MirrorCamera {
    pub eye: Vec3,
    matrix: Mat4,
    pass: macroquad::texture::RenderPass,
}
impl Camera for MirrorCamera {
    fn matrix(&self) -> Mat4 {
        self.matrix
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        Some(self.pass.clone())
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aperture_corners_fill_target_with_parallax_and_correct_handedness() {
        for (center, right, up, eye) in [
            (vec3(0., 2.5, -8.), Vec3::X, Vec3::Y, vec3(3., 1.7, 10.)),
            (Vec3::ZERO, Vec3::Z, Vec3::Y, vec3(-5., 2., 3.)),
        ] {
            let p = MirrorPlane::new(center, right, up, vec2(10., 5.)).unwrap();
            let (reflected, m) = p.projection(eye, 100.).unwrap();
            assert!(
                ((reflected - center).dot(p.normal) + (eye - center).dot(p.normal)).abs() < 1e-5
            );
            let expected = [vec2(1., -1.), vec2(-1., -1.), vec2(-1., 1.), vec2(1., 1.)];
            // Use exact aperture corners (draw corners have a small depth bias).
            for (corner, want) in p.corners().into_iter().zip(expected) {
                let clip = m * (corner - p.normal * 0.001).extend(1.);
                let ndc = clip.truncate() / clip.w;
                assert!((ndc.truncate() - want).length() < 1e-4);
            }
        }
    }
    #[test]
    fn bad_geometry_and_backside_are_refused() {
        assert!(MirrorPlane::new(Vec3::ZERO, Vec3::ZERO, Vec3::Y, Vec2::ONE).is_err());
        assert!(MirrorPlane::new(Vec3::ZERO, Vec3::X, Vec3::X, Vec2::ONE).is_err());
        assert!(MirrorPlane::new(Vec3::ZERO, Vec3::X, Vec3::Y, vec2(f32::NAN, 1.)).is_err());
        let p = MirrorPlane::new(Vec3::ZERO, Vec3::X, Vec3::Y, Vec2::ONE).unwrap();
        for eye in [Vec3::ZERO, -Vec3::Z, vec3(f32::NAN, 0., 1.)] {
            assert!(p.projection(eye, 100.).is_none());
        }
        assert!(p.projection(Vec3::Z, 0.5).is_none());
        assert!(p.projection(Vec3::Z, f32::INFINITY).is_none());
    }
    #[test]
    fn near_plane_clips_geometry_behind_mirror() {
        let p = MirrorPlane::new(Vec3::ZERO, Vec3::X, Vec3::Y, Vec2::ONE).unwrap();
        let (_, m) = p.projection(vec3(0., 0., 3.), 50.).unwrap();
        let front = m * vec4(0., 0., 1., 1.);
        let back = m * vec4(0., 0., -1., 1.);
        assert!(front.z / front.w >= -1.);
        assert!(back.z / back.w < -1.);
    }
}
