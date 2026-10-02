//! A camera description shared by the renderer, the sky and the HUD.
use macroquad::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// A far/near ratio above this starves a 24-bit depth buffer: the resolution at distance `d` is about
/// `d^2 / (near * 2^24)` ([`depth_resolution`](super::lint::depth_resolution)), so a 0.1 / 700 camera
/// cannot tell two surfaces 3 cm apart at 700 m and coplanar-ish ground details (kerbs, decals, road
/// markings) shimmer in the distance. See [`View::camera_checked`] and [`depth_ratio_warning`].
pub const DEPTH_RATIO_WARN: f32 = 3000.;

/// A one-line warning when `far / near` exceeds [`DEPTH_RATIO_WARN`] (or the planes are unusable), else
/// `None`. Pure, so a test can assert a game's chosen planes.
pub fn depth_ratio_warning(near: f32, far: f32) -> Option<String> {
    if !(near > 0. && far > near) {
        return Some(format!(
            "camera planes near {near} / far {far} are invalid: need 0 < near < far"
        ));
    }
    let ratio = far / near;
    (ratio > DEPTH_RATIO_WARN).then(|| {
        format!(
            "camera near {near} / far {far} is a depth ratio of {ratio:.0} (> {DEPTH_RATIO_WARN:.0}): \
             distant surfaces closer together than {:.3} at the far plane z-fight. Raise near or lower far",
            super::lint::depth_resolution(near, far)
        )
    })
}

/// Where the camera is and how it looks this frame. Angles follow [`Controller`](crate::viewer::controller::Controller):
/// yaw 0 faces -Z, positive yaw turns towards +X, pitch is up-positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Eye position in world space.
    pub eye: Vec3,
    /// Yaw in radians.
    pub yaw: f32,
    /// Pitch in radians.
    pub pitch: f32,
    /// Roll about the view axis in radians (screen shake, wall-run tilt).
    pub roll: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
}

impl View {
    /// A first-person view: `yaw`/`pitch` from the player controller, the standard 70 degree FOV.
    pub fn first_person(eye: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            eye,
            yaw,
            pitch,
            roll: 0.,
            fov: 70f32.to_radians(),
        }
    }
    /// Unit vector the camera looks along.
    pub fn dir(&self) -> Vec3 {
        vec3(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    /// Unit vector to the camera's right (never NaN, even when looking straight up or down).
    pub fn right(&self) -> Vec3 {
        let r = self.dir().cross(Vec3::Y).normalize_or_zero();
        if r == Vec3::ZERO {
            Vec3::X
        } else {
            r
        }
    }
    /// Unit vector to the camera's up, rolled about the view axis.
    pub fn up(&self) -> Vec3 {
        let right = self.right();
        let up = right.cross(self.dir()).normalize_or_zero();
        let (s, c) = self.roll.sin_cos();
        up * c + right * s
    }
    /// The macroquad camera for this view.
    ///
    /// Watch the `far / near` ratio: depth precision is spent mostly near the camera, so a ratio over about
    /// 3000 (0.1 / 700 is 7000) makes distant coplanar-ish surfaces fight. This method uses the planes
    /// exactly as given; [`View::camera_checked`] is the same camera plus a one-time warning.
    pub fn camera(&self, near: f32, far: f32) -> Camera3D {
        Camera3D {
            position: self.eye,
            target: self.eye + self.dir(),
            up: self.up(),
            fovy: self.fov,
            z_near: near,
            z_far: far,
            ..Default::default()
        }
    }
    /// [`View::camera`] that prints one warning to stderr (once per process, however often it is called)
    /// when `far / near` exceeds [`DEPTH_RATIO_WARN`]. The camera returned is identical either way; use it
    /// in the frame loop to find out about a depth-precision trap during development.
    pub fn camera_checked(&self, near: f32, far: f32) -> Camera3D {
        static WARNED: AtomicBool = AtomicBool::new(false);
        if let Some(msg) = depth_ratio_warning(near, far) {
            if !WARNED.swap(true, Ordering::Relaxed) {
                eprintln!("warning: {msg}");
            }
        }
        self.camera(near, far)
    }
    /// The same orientation with the camera at the origin: draw a sky dome with this so it never
    /// moves closer.
    pub fn sky_camera(&self) -> Camera3D {
        Camera3D {
            position: Vec3::ZERO,
            target: self.dir(),
            up: self.up(),
            fovy: self.fov,
            z_near: 1.,
            z_far: 500.,
            ..Default::default()
        }
    }
    /// Project a world point to pixels of a `width` x `height` screen; `None` behind the camera.
    /// Use it to anchor floating text, health bars and markers to world positions.
    pub fn project(&self, p: Vec3, width: f32, height: f32) -> Option<Vec2> {
        let forward = self.dir();
        let right = self.right();
        let up = right.cross(forward).normalize_or_zero();
        let rel = p - self.eye;
        let depth = rel.dot(forward);
        if depth < 0.15 {
            return None;
        }
        let t = (self.fov * 0.5).tan();
        let x = rel.dot(right) / (depth * t * (width / height));
        let y = rel.dot(up) / (depth * t);
        Some(vec2((x * 0.5 + 0.5) * width, (0.5 - y * 0.5) * height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_straight_ahead_projects_to_the_screen_centre_and_behind_is_none() {
        let view = View::first_person(vec3(0., 1.7, 0.), 0., 0.);
        let ahead = view.project(vec3(0., 1.7, -10.), 1280., 720.).unwrap();
        assert!((ahead - vec2(640., 360.)).length() < 0.01, "{ahead}");
        assert!(view.project(vec3(0., 1.7, 10.), 1280., 720.).is_none());
        let right = view.project(vec3(2., 1.7, -10.), 1280., 720.).unwrap();
        assert!(
            right.x > 640. && (right.y - 360.).abs() < 0.01,
            "+x is to the right of the view: {right}"
        );
        let up = view.project(vec3(0., 3.7, -10.), 1280., 720.).unwrap();
        assert!(up.y < 360., "higher points have smaller screen y");
    }

    #[test]
    fn yaw_turns_towards_plus_x_and_the_basis_is_orthonormal_at_every_pitch() {
        let view = View::first_person(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.);
        assert!((view.dir() - Vec3::X).length() < 1e-5);
        for pitch in [
            -1.5,
            -0.7,
            0.,
            0.7,
            1.5,
            std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2,
        ] {
            let v = View {
                pitch,
                roll: 0.3,
                ..View::first_person(Vec3::ZERO, 0.4, pitch)
            };
            let (f, r, u) = (v.dir(), v.right(), v.up());
            assert!(
                f.is_finite() && r.is_finite() && u.is_finite(),
                "pitch {pitch}"
            );
            assert!(
                f.dot(r).abs() < 1e-3 && (u.length() - 1.).abs() < 1e-3,
                "pitch {pitch}"
            );
        }
    }

    #[test]
    fn a_depth_ratio_past_3000_warns_and_the_checked_camera_is_the_plain_camera() {
        assert!(depth_ratio_warning(0.1, 250.).is_none());
        assert!(
            depth_ratio_warning(0.1, 300.).is_none(),
            "exactly 3000 is fine"
        );
        let w = depth_ratio_warning(0.1, 700.).unwrap();
        assert!(w.contains("7000"), "{w}");
        assert!(depth_ratio_warning(0.05, 400.).is_some());
        assert!(depth_ratio_warning(0., 100.).is_some() && depth_ratio_warning(5., 1.).is_some());
        let view = View::first_person(vec3(1., 2., 3.), 0.3, 0.1);
        let (plain, checked) = (view.camera(0.1, 700.), view.camera_checked(0.1, 700.));
        let _ = view.camera_checked(0.1, 700.); // a second call must not warn again (and must not panic)
        assert_eq!((plain.z_near, plain.z_far), (checked.z_near, checked.z_far));
        assert_eq!(
            (plain.position, plain.target, plain.up),
            (checked.position, checked.target, checked.up)
        );
    }

    #[test]
    fn the_sky_camera_sits_at_the_origin_with_the_same_orientation() {
        let view = View::first_person(vec3(5., 9., 2.), 1., 0.2);
        let sky = view.sky_camera();
        assert_eq!(sky.position, Vec3::ZERO);
        assert!((sky.target - view.dir()).length() < 1e-6);
        assert_eq!(view.camera(0.1, 100.).position, view.eye);
    }
}
