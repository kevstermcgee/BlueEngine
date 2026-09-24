//! Camera placement is independent of movement; gameplay rays still start at the player.
use super::{
    controller::{Collider, Controller},
    room::Room,
};
use crate::math::{Ray, V};
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perspective {
    #[default]
    First,
    Third,
}
impl Perspective {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::First => Self::Third,
            Self::Third => Self::First,
        };
    }
    pub fn view(self, player: &Controller, room: &Room) -> View {
        if self == Self::First {
            return View {
                eye: player.position,
                target: player.position + player.direction(),
                show_body: false,
            };
        }
        let anchor = player.position;
        let right = V(player.yaw.cos(), 0., player.yaw.sin());
        let small = player.character_kind() == super::controller::CharacterKind::Feta;
        // Rat-sized passages need a low, centered boom. Pitch changes the view,
        // not the boom height, so looking around cannot swing it into a tabletop.
        let desired = if small {
            anchor - V(player.yaw.sin(), 0., -player.yaw.cos()) * 1.05 + V(0., 0.06, 0.)
        } else {
            anchor - player.direction() * 2.7 + right * 0.65 + V(0., 0.28, 0.)
        };
        let radius = if small { 0.07 } else { 0.18 };
        let delta = desired - anchor;
        let distance = delta.length();
        let ray = Ray {
            o: anchor,
            d: delta / distance,
        };
        // Expanded boxes sweep a small sphere; this protects the near plane at corners.
        let mut limit = distance;
        for c in &room.colliders {
            if let Some(t) = entry(ray, c, radius) {
                limit = limit.min((t - 0.025).max(0.));
            }
        }
        // Decorative scene geometry can also obstruct the camera boom.
        if let Some(hit) = room.hit(ray, distance) {
            limit = limit.min((hit.t - radius - 0.02).max(0.));
        }
        let eye = anchor + ray.d * limit;
        View {
            eye,
            target: if small {
                eye + player.direction() * 4.
            } else {
                anchor + player.direction() * 4.
            },
            show_body: limit > if small { 0.16 } else { 0.55 },
        }
    }
}
/// Presentation-only boom distance: immediate collision protection, gradual release.
/// Follows the player directly; smoothing never adds movement or look latency.
#[derive(Default)]
pub struct CameraRig {
    distance: Option<f32>,
    mode: Option<(Perspective, bool)>,
}
impl CameraRig {
    pub fn advance(&mut self, mode: Perspective, player: &Controller, room: &Room, dt: f32) {
        let small = player.character_kind() == super::controller::CharacterKind::Feta;
        let wanted = (mode.view(player, room).eye - player.position).length();
        let next = match self.distance {
            Some(previous) if self.mode == Some((mode, small)) => {
                let dt = if dt.is_finite() {
                    dt.clamp(0., 0.1)
                } else {
                    0.
                };
                if wanted < previous {
                    wanted
                } else {
                    previous + (wanted - previous) * (1. - (-6. * dt).exp())
                }
            }
            _ => wanted,
        };
        self.distance = Some(next);
        self.mode = Some((mode, small));
    }
    /// Recheck collision for the current/interpolated pose even between updates.
    pub fn view(&self, mode: Perspective, player: &Controller, room: &Room) -> View {
        let mut view = mode.view(player, room);
        let small = player.character_kind() == super::controller::CharacterKind::Feta;
        if mode == Perspective::Third && self.mode == Some((mode, small)) {
            let delta = view.eye - player.position;
            let length = delta.length();
            let distance = self.distance.unwrap_or(length).min(length);
            if length > 0.0001 {
                view.eye = player.position + delta * (distance / length);
            }
            if small {
                view.target = view.eye + player.direction() * 4.;
            }
            view.show_body = distance > if small { 0.16 } else { 0.55 };
        }
        view
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: V,
    pub target: V,
    pub show_body: bool,
}
impl View {
    pub fn aim(self, player: &Controller, room: &Room) -> Ray {
        let ray = Ray {
            o: self.eye,
            d: (self.target - self.eye).norm(),
        };
        let point = room.hit(ray, 40.).map_or(self.target, |h| h.p);
        Ray {
            o: player.position,
            d: (point - player.position).norm(),
        }
    }
}
fn entry(ray: Ray, c: &Collider, radius: f32) -> Option<f32> {
    let mut near = 0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let lo = c.min.axis(axis) - radius;
        let hi = c.max.axis(axis) + radius;
        let o = ray.o.axis(axis);
        let d = ray.d.axis(axis);
        if d.abs() < 1e-6 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let a = (lo - o) / d;
            let b = (hi - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.).then_some(near)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggle_preserves_player_and_first_person_ray() {
        let room = super::super::room::build().unwrap();
        let p = Controller::default();
        let mut mode = Perspective::default();
        let first = mode.view(&p, &room);
        assert_eq!(first.eye, p.position);
        assert!((first.aim(&p, &room).d - p.direction()).length() < 0.0001);
        mode.toggle();
        assert!(mode.view(&p, &room).eye != p.position);
        mode.toggle();
        assert_eq!(mode.view(&p, &room).eye, p.position);
    }
    #[test]
    fn boom_stays_inside_room_at_walls_and_extreme_pitch() {
        let room = super::super::room::build().unwrap();
        for yaw in [0., std::f32::consts::FRAC_PI_2, std::f32::consts::PI, 4.71] {
            for pitch in [-1.5, 0., 1.5] {
                let mut p = Controller::default();
                p.position = V(0., 1.68, 5.65);
                p.yaw = yaw;
                p.pitch = pitch;
                let v = Perspective::Third.view(&p, &room);
                assert!(v.eye.finite());
                assert!(v.eye.2 < 5.9 && v.eye.1 > 0.1 && v.eye.1 < 3.6);
                assert_eq!(v.aim(&p, &room).o, p.position);
            }
        }
    }
    #[test]
    fn sweep_catches_corner_and_parallel_cases() {
        let c = Collider {
            min: V(0., 0., 0.),
            max: V(1., 1., 1.),
        };
        assert!(entry(
            Ray {
                o: V(-0.1, 0.5, -2.),
                d: V(0., 0., 1.)
            },
            &c,
            0.18
        )
        .is_some());
        assert!(entry(
            Ray {
                o: V(-0.3, 0.5, -2.),
                d: V(0., 0., 1.)
            },
            &c,
            0.18
        )
        .is_none());
    }
    #[test]
    fn shoulder_aim_converges_on_visible_surface_from_player() {
        let room = super::super::room::build().unwrap();
        let mut p = Controller::default();
        p.position = V(-3.3, 1.34, -1.6);
        p.yaw = -std::f32::consts::FRAC_PI_2;
        p.pitch = 0.;
        let v = Perspective::Third.view(&p, &room);
        let aim = v.aim(&p, &room);
        let camera_hit = room
            .world
            .hit(
                Ray {
                    o: v.eye,
                    d: (v.target - v.eye).norm(),
                },
                40.,
                false,
            )
            .unwrap();
        let player_hit = room.world.hit(aim, 40., false).unwrap();
        assert!((camera_hit.p - player_hit.p).length() < 0.01);
        assert!(player_hit.t < super::super::wrench::REACH);
    }
}
