//! Local weapon selection and pistol shots, independent of rendering and input APIs.
use super::{controller::CharacterKind, room::Room, wrench::Impact};
use crate::math::Ray;

pub const PISTOL_RANGE: f32 = 40.;
pub const SHOT_INTERVAL: f32 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weapon {
    #[default]
    Wrench,
    Pistol,
}

/// Unlimited for the prototype; limited ammunition counts accepted shots only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ammo {
    #[default]
    Unlimited,
    Limited(u32),
}

#[derive(Default)]
pub struct Pistol {
    pub ammo: Ammo,
    pub shots: u32,
    pub impact: Option<Impact>,
    cooldown: f32,
    pub flash: f32,
}
impl Pistol {
    pub fn recoil(&self) -> f32 {
        (self.cooldown / SHOT_INTERVAL).powi(3)
    }
    /// Advance only during gameplay. Switching weapons does not reset cooldown.
    pub fn tick(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        self.cooldown = (self.cooldown - dt).max(0.);
        self.flash = (self.flash - dt).max(0.);
        if let Some(hit) = &mut self.impact {
            hit.age += dt;
        }
        if self.impact.as_ref().is_some_and(|h| h.age > 0.65) {
            self.impact = None;
        }
    }
    /// One accepted click produces one closest-surface hitscan; misses also spend ammo.
    pub fn fire(&mut self, allowed: bool, room: &Room, ray: Ray) -> bool {
        if !allowed
            || self.cooldown > 0.
            || self.ammo == Ammo::Limited(0)
            || !ray.o.finite()
            || !ray.d.finite()
            || ray.d.length() < 0.001
        {
            return false;
        }
        if let Ammo::Limited(count) = &mut self.ammo {
            *count -= 1;
        }
        self.cooldown = SHOT_INTERVAL;
        self.flash = 0.055;
        self.shots = self.shots.saturating_add(1);
        self.impact = room
            .hit(
                Ray {
                    o: ray.o,
                    d: ray.d.norm(),
                },
                PISTOL_RANGE,
            )
            .map(|hit| Impact {
                point: hit.p,
                normal: hit.n,
                age: 0.,
                label: room
                    .entities
                    .iter()
                    .find(|e| e.bounds.contains(hit.p))
                    .map_or_else(|| "Surface".into(), |e| e.label.clone()),
            });
        true
    }
}

#[derive(Default)]
pub struct Loadout {
    pub selected: Weapon,
    pub pistol: Pistol,
}
impl Loadout {
    pub fn can_use(kind: CharacterKind, active: bool, carrying: bool) -> bool {
        kind == CharacterKind::Scientist && active && !carrying
    }
    /// Either scroll direction cycles two slots once per nonzero wheel event/frame.
    pub fn scroll(
        &mut self,
        delta: f32,
        kind: CharacterKind,
        active: bool,
        carrying: bool,
    ) -> bool {
        if !delta.is_finite() || delta == 0. || !Self::can_use(kind, active, carrying) {
            return false;
        }
        self.selected = match self.selected {
            Weapon::Wrench => Weapon::Pistol,
            Weapon::Pistol => Weapon::Wrench,
        };
        true
    }
}

/// Deterministic countdown in whole simulation ticks (inspired by RedEngine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickCooldown(pub u32);

impl TickCooldown {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn start(&mut self, ticks: u32) {
        self.0 = ticks;
    }

    pub fn tick(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }

    pub fn ready(&self) -> bool {
        self.0 == 0
    }
}

/// Deterministic melee swing state machine in fixed ticks: windup -> strike -> recover.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeleeSwingState {
    ticks: Option<u32>,
    struck: bool,
    windup_ticks: u32,
    total_ticks: u32,
}

impl MeleeSwingState {
    pub fn new(windup_ticks: u32, total_ticks: u32) -> Self {
        Self {
            ticks: None,
            struck: false,
            windup_ticks,
            total_ticks,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.ticks.is_none()
    }

    pub fn start(&mut self) -> bool {
        if self.ticks.is_some() {
            return false;
        }
        self.ticks = Some(0);
        self.struck = false;
        true
    }

    pub fn cancel(&mut self) {
        self.ticks = None;
    }

    /// Advance one tick. Returns true on the exact tick where the strike lands.
    pub fn tick(&mut self) -> bool {
        let Some(t) = self.ticks else { return false };
        let t = t + 1;
        let strike = t >= self.windup_ticks && !self.struck;
        self.struck |= strike;
        self.ticks = if t >= self.total_ticks { None } else { Some(t) };
        strike
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{math::V, viewer::room};
    #[test]
    fn selection_starts_with_wrench_and_respects_role_pause_and_carrying() {
        let mut l = Loadout::default();
        assert_eq!(l.selected, Weapon::Wrench);
        for (kind, active, carrying) in [
            (CharacterKind::Feta, true, false),
            (CharacterKind::Scientist, false, false),
            (CharacterKind::Scientist, true, true),
        ] {
            assert!(!l.scroll(1., kind, active, carrying));
        }
        assert!(l.scroll(1., CharacterKind::Scientist, true, false));
        assert_eq!(l.selected, Weapon::Pistol);
        assert!(!l.scroll(0., CharacterKind::Scientist, true, false));
        assert!(l.scroll(-1., CharacterKind::Scientist, true, false));
        assert_eq!(l.selected, Weapon::Wrench);
    }
    #[test]
    fn shots_hit_closest_surface_and_infinite_ammo_never_runs_out() {
        let room = room::build().unwrap();
        let ray = Ray {
            o: V(0., 1.4, 0.),
            d: V(-1., 0., 0.),
        };
        let expected = room.hit(ray, PISTOL_RANGE).unwrap().p;
        let mut p = Pistol::default();
        assert!(!p.fire(false, &room, ray));
        for _ in 0..200 {
            assert!(p.fire(true, &room, ray));
            assert!((p.impact.as_ref().unwrap().point - expected).length() < 0.001);
            assert!(!p.fire(true, &room, ray));
            p.tick(SHOT_INTERVAL);
        }
        assert_eq!(p.shots, 200);
        assert_eq!(p.ammo, Ammo::Unlimited);
    }
    #[test]
    fn limited_ammo_and_misses_and_frame_independent_recovery() {
        let room = room::build().unwrap();
        let miss = Ray {
            o: V(0., 30., 0.),
            d: V(0., 1., 0.),
        };
        for fps in [30, 60, 144] {
            let mut p = Pistol {
                ammo: Ammo::Limited(2),
                ..Default::default()
            };
            assert!(p.fire(true, &room, miss));
            assert!(p.impact.is_none());
            for _ in 0..fps {
                p.tick(1. / fps as f32);
            }
            assert_eq!(p.recoil(), 0.);
            assert!(p.fire(true, &room, miss));
            p.tick(1.);
            assert!(!p.fire(true, &room, miss));
            assert_eq!(p.shots, 2);
        }
    }

    #[test]
    fn tick_combat_timing_and_melee_windup_strike() {
        let mut cooldown = TickCooldown::new();
        assert!(cooldown.ready());
        cooldown.start(3);
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(!cooldown.ready());
        cooldown.tick();
        assert!(cooldown.ready());

        let mut swing = MeleeSwingState::new(2, 5);
        assert!(swing.is_idle());
        assert!(swing.start());
        assert!(!swing.is_idle());
        assert!(!swing.tick()); // tick 1: windup
        assert!(swing.tick());  // tick 2: strike lands!
        assert!(!swing.tick()); // tick 3: recover
        assert!(!swing.tick()); // tick 4: recover
        assert!(!swing.tick()); // tick 5: end of swing
        assert!(swing.is_idle());
    }
}
