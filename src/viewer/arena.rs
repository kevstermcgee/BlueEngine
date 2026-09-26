//! Rendering-independent building blocks for momentum-driven arena shooters.
//!
//! The ordinary [`super::controller::Controller`] intentionally favors predictable,
//! approachable movement. This module supplies an opt-in controller for games where
//! acceleration, air steering, conserved jump momentum and vertical launch routes are
//! core mechanics. It also owns small deterministic projectile, pickup and frag-match
//! primitives so game crates do not need to rebuild them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::math::V;

use super::controller::Collider;

pub const ARENA_TICK_RATE: u32 = 60;
pub const DEFAULT_ARENA_FOV: f32 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaMovementConfig {
    pub ground_speed: f32,
    pub ground_acceleration: f32,
    pub air_acceleration: f32,
    pub air_control_speed: f32,
    pub friction: f32,
    pub stop_speed: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    pub radius: f32,
    pub height: f32,
    pub eye_height: f32,
}

impl Default for ArenaMovementConfig {
    fn default() -> Self {
        Self {
            ground_speed: 10.5,
            ground_acceleration: 52.0,
            air_acceleration: 13.5,
            air_control_speed: 10.5,
            friction: 7.0,
            stop_speed: 2.5,
            gravity: 24.0,
            jump_speed: 8.2,
            radius: 0.34,
            height: 1.72,
            eye_height: 1.56,
        }
    }
}

impl ArenaMovementConfig {
    pub fn validate(self) -> crate::Result<Self> {
        let values = [
            self.ground_speed,
            self.ground_acceleration,
            self.air_acceleration,
            self.air_control_speed,
            self.friction,
            self.stop_speed,
            self.gravity,
            self.jump_speed,
            self.radius,
            self.height,
            self.eye_height,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
            || self.eye_height >= self.height
        {
            return Err(
                "arena movement values must be finite, positive, and physically valid".into(),
            );
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaInput {
    pub forward: f32,
    pub right: f32,
    /// Held jump intentionally supports immediate re-jumping on landing.
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArenaBody {
    /// Eye position, matching the conventional first-person camera origin.
    pub position: V,
    pub velocity: V,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub config: ArenaMovementConfig,
}

impl ArenaBody {
    pub fn spawn(feet: V, yaw: f32, config: ArenaMovementConfig) -> crate::Result<Self> {
        let config = config.validate()?;
        if !feet.finite() || !yaw.is_finite() {
            return Err("arena spawn must be finite".into());
        }
        Ok(Self {
            position: feet + V(0.0, config.eye_height, 0.0),
            velocity: V::ZERO,
            yaw,
            pitch: 0.0,
            grounded: true,
            config,
        })
    }

    pub fn feet(self) -> V {
        self.position - V(0.0, self.config.eye_height, 0.0)
    }

    pub fn speed(self) -> f32 {
        V(self.velocity.0, 0.0, self.velocity.2).length()
    }

    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f32) {
        if dx.is_finite() && dy.is_finite() && sensitivity.is_finite() {
            self.yaw = (self.yaw + dx * sensitivity).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch - dy * sensitivity).clamp(-1.5, 1.5);
        }
    }

    pub fn launch(&mut self, velocity: V) {
        if velocity.finite() {
            self.velocity = velocity;
            self.grounded = false;
        }
    }

    pub fn step(&mut self, input: ArenaInput, dt: f32, colliders: &[Collider]) {
        if !dt.is_finite() || dt <= 0.0 || !input.forward.is_finite() || !input.right.is_finite() {
            return;
        }
        let steps = (dt.min(0.1) / 0.004).ceil().max(1.0) as usize;
        let h = dt.min(0.1) / steps as f32;
        for _ in 0..steps {
            self.substep(input, h, colliders);
        }
    }

    fn substep(&mut self, input: ArenaInput, dt: f32, colliders: &[Collider]) {
        let mut wish = V(
            self.yaw.sin() * input.forward + self.yaw.cos() * input.right,
            0.0,
            -self.yaw.cos() * input.forward + self.yaw.sin() * input.right,
        );
        let magnitude = (input.forward * input.forward + input.right * input.right)
            .sqrt()
            .min(1.0);
        if wish.length() > 0.0001 {
            wish = wish.norm();
        }

        if self.grounded {
            if input.jump {
                self.velocity.1 = self.config.jump_speed;
                self.grounded = false;
            } else {
                self.apply_friction(dt);
            }
        }
        let wish_speed = if self.grounded {
            self.config.ground_speed * magnitude
        } else {
            self.config.air_control_speed * magnitude
        };
        let acceleration = if self.grounded {
            self.config.ground_acceleration
        } else {
            self.config.air_acceleration
        };
        self.accelerate(wish, wish_speed, acceleration, dt);
        self.velocity.1 -= self.config.gravity * dt;

        let feet = self.feet();
        let dx = V(self.velocity.0 * dt, 0.0, 0.0);
        if self.blocked(feet + dx, colliders) {
            self.velocity.0 = 0.0;
        } else {
            self.position.0 += dx.0;
        }
        let feet = self.feet();
        let dz = V(0.0, 0.0, self.velocity.2 * dt);
        if self.blocked(feet + dz, colliders) {
            self.velocity.2 = 0.0;
        } else {
            self.position.2 += dz.2;
        }

        let old_feet = self.feet();
        let next_feet = old_feet + V(0.0, self.velocity.1 * dt, 0.0);
        self.grounded = false;
        if self.velocity.1 > 0.0 {
            let ceiling = colliders
                .iter()
                .filter(|c| c.overlaps_xz(self.position, self.config.radius))
                .filter(|c| old_feet.1 + self.config.height <= c.min.1 + 0.001)
                .filter(|c| next_feet.1 + self.config.height >= c.min.1)
                .map(|c| c.min.1 - self.config.height)
                .reduce(f32::min);
            if let Some(y) = ceiling {
                self.position.1 = y + self.config.eye_height;
                self.velocity.1 = 0.0;
            } else {
                self.position.1 = next_feet.1 + self.config.eye_height;
            }
        } else {
            let support = colliders
                .iter()
                .filter(|c| c.overlaps_xz(self.position, self.config.radius))
                .filter(|c| old_feet.1 >= c.max.1 - 0.001 && next_feet.1 <= c.max.1)
                .map(|c| c.max.1)
                .fold(0.0_f32, f32::max);
            if next_feet.1 <= support {
                self.position.1 = support + self.config.eye_height;
                self.velocity.1 = 0.0;
                self.grounded = true;
            } else {
                self.position.1 = next_feet.1 + self.config.eye_height;
            }
        }
    }

    fn blocked(&self, feet: V, colliders: &[Collider]) -> bool {
        colliders.iter().any(|collider| {
            collider.overlaps_body(
                feet + V(0.0, self.config.eye_height, 0.0),
                feet.1,
                self.config.height,
                self.config.radius,
            )
        })
    }

    fn apply_friction(&mut self, dt: f32) {
        let speed = self.speed();
        if speed <= 0.0001 {
            return;
        }
        let drop = self.config.stop_speed.max(speed) * self.config.friction * dt;
        let scale = (speed - drop).max(0.0) / speed;
        self.velocity.0 *= scale;
        self.velocity.2 *= scale;
    }

    fn accelerate(&mut self, direction: V, wish_speed: f32, acceleration: f32, dt: f32) {
        if wish_speed <= 0.0 {
            return;
        }
        let current = self.velocity.dot(direction);
        let add = wish_speed - current;
        if add > 0.0 {
            let amount = (acceleration * wish_speed * dt).min(add);
            self.velocity = self.velocity + direction * amount;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LaunchVolume {
    pub min: V,
    pub max: V,
    pub velocity: V,
}

impl LaunchVolume {
    pub fn activate(self, body: &mut ArenaBody) -> bool {
        let feet = body.feet();
        if feet.0 >= self.min.0
            && feet.0 <= self.max.0
            && feet.1 >= self.min.1
            && feet.1 <= self.max.1
            && feet.2 >= self.min.2
            && feet.2 <= self.max.2
        {
            body.launch(self.velocity);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickupKind {
    Health,
    Armor,
    Ammo,
    Powerup,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedPickup {
    pub id: String,
    pub kind: PickupKind,
    pub position: V,
    pub amount: u16,
    pub radius: f32,
    pub respawn_ticks: u32,
    remaining_ticks: u32,
}

impl TimedPickup {
    pub fn new(
        id: impl Into<String>,
        kind: PickupKind,
        position: V,
        amount: u16,
        respawn_ticks: u32,
    ) -> crate::Result<Self> {
        let id = id.into();
        if id.trim().is_empty() || !position.finite() || amount == 0 || respawn_ticks == 0 {
            return Err("pickup requires an id, finite position, amount and respawn time".into());
        }
        Ok(Self {
            id,
            kind,
            position,
            amount,
            radius: 0.75,
            respawn_ticks,
            remaining_ticks: 0,
        })
    }

    pub fn available(&self) -> bool {
        self.remaining_ticks == 0
    }
    pub fn step(&mut self) {
        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
    }
    pub fn collect(&mut self, position: V) -> Option<u16> {
        if self.available() && (position - self.position).length() <= self.radius {
            self.remaining_ticks = self.respawn_ticks;
            Some(self.amount)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileSpec {
    pub speed: f32,
    pub direct_damage: f32,
    pub splash_damage: f32,
    pub splash_radius: f32,
    pub lifetime_ticks: u32,
}

impl ProjectileSpec {
    pub fn splash_at(self, distance: f32) -> f32 {
        if !distance.is_finite() || distance < 0.0 || distance >= self.splash_radius {
            return 0.0;
        }
        self.splash_damage * (1.0 - distance / self.splash_radius)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub owner: u64,
    pub position: V,
    pub previous_position: V,
    pub velocity: V,
    pub remaining_ticks: u32,
    pub spec: ProjectileSpec,
}

impl Projectile {
    pub fn spawn(
        owner: u64,
        position: V,
        direction: V,
        spec: ProjectileSpec,
    ) -> crate::Result<Self> {
        if !position.finite()
            || !direction.finite()
            || direction.length() < 0.001
            || !spec.speed.is_finite()
            || spec.speed <= 0.0
            || spec.lifetime_ticks == 0
        {
            return Err("projectile definition and transform must be valid".into());
        }
        Ok(Self {
            owner,
            position,
            previous_position: position,
            velocity: direction.norm() * spec.speed,
            remaining_ticks: spec.lifetime_ticks,
            spec,
        })
    }
    pub fn step(&mut self, dt: f32) -> bool {
        if self.remaining_ticks == 0 || !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        self.previous_position = self.position;
        self.position = self.position + self.velocity * dt;
        self.remaining_ticks -= 1;
        self.remaining_ticks > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FragMatchConfig {
    pub frag_limit: u32,
    pub time_limit_ticks: u64,
    pub respawn_ticks: u32,
}

impl Default for FragMatchConfig {
    fn default() -> Self {
        Self {
            frag_limit: 20,
            time_limit_ticks: 8 * 60 * ARENA_TICK_RATE as u64,
            respawn_ticks: 2 * ARENA_TICK_RATE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaCombatant {
    pub health: u16,
    pub armor: u16,
    pub frags: i32,
    pub deaths: u32,
    pub respawn_ticks: u32,
}

pub struct FragMatch {
    pub config: FragMatchConfig,
    pub tick: u64,
    pub players: BTreeMap<u64, ArenaCombatant>,
}

impl FragMatch {
    pub fn new(config: FragMatchConfig) -> crate::Result<Self> {
        if config.frag_limit == 0 || config.time_limit_ticks == 0 || config.respawn_ticks == 0 {
            return Err("frag match limits must be nonzero".into());
        }
        Ok(Self {
            config,
            tick: 0,
            players: BTreeMap::new(),
        })
    }
    pub fn join(&mut self, id: u64) -> bool {
        self.players
            .insert(
                id,
                ArenaCombatant {
                    health: 100,
                    armor: 0,
                    frags: 0,
                    deaths: 0,
                    respawn_ticks: 0,
                },
            )
            .is_none()
    }
    pub fn damage(&mut self, attacker: u64, victim: u64, damage: f32) -> bool {
        if !damage.is_finite() || damage <= 0.0 || !self.players.contains_key(&attacker) {
            return false;
        }
        let Some(target) = self.players.get_mut(&victim) else {
            return false;
        };
        if target.health == 0 {
            return false;
        }
        let incoming = damage.ceil().min(u16::MAX as f32) as u16;
        let absorbed = target.armor.min(incoming / 2);
        target.armor -= absorbed;
        target.health = target.health.saturating_sub(incoming - absorbed);
        if target.health == 0 {
            target.deaths += 1;
            target.respawn_ticks = self.config.respawn_ticks;
            if let Some(killer) = self.players.get_mut(&attacker) {
                killer.frags += if attacker == victim { -1 } else { 1 };
            }
            return true;
        }
        false
    }
    pub fn step(&mut self) -> Vec<u64> {
        self.tick += 1;
        let mut respawns = Vec::new();
        for (&id, player) in &mut self.players {
            if player.health == 0 {
                player.respawn_ticks = player.respawn_ticks.saturating_sub(1);
                if player.respawn_ticks == 0 {
                    player.health = 100;
                    player.armor = 0;
                    respawns.push(id);
                }
            }
        }
        respawns
    }
    pub fn winner(&self) -> Option<u64> {
        let limit_reached = self
            .players
            .values()
            .any(|p| p.frags >= self.config.frag_limit as i32);
        if !limit_reached && self.tick < self.config.time_limit_ticks {
            return None;
        }
        self.players
            .iter()
            .max_by_key(|(_, p)| p.frags)
            .map(|(&id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_controller_reaches_speed_and_preserves_jump_momentum() {
        let mut body = ArenaBody::spawn(V::ZERO, 0.0, ArenaMovementConfig::default()).unwrap();
        for _ in 0..60 {
            body.step(
                ArenaInput {
                    forward: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
                &[],
            );
        }
        assert!((body.speed() - 10.5).abs() < 0.1);
        let before = body.speed();
        body.step(
            ArenaInput {
                forward: 1.0,
                jump: true,
                right: 0.0,
            },
            1.0 / 60.0,
            &[],
        );
        assert!(!body.grounded);
        assert!(body.speed() >= before - 0.1);
    }

    #[test]
    fn launch_pickup_projectile_and_splash_are_deterministic() {
        let mut body = ArenaBody::spawn(V::ZERO, 0.0, ArenaMovementConfig::default()).unwrap();
        let pad = LaunchVolume {
            min: V(-1.0, -0.1, -1.0),
            max: V(1.0, 0.1, 1.0),
            velocity: V(4.0, 12.0, 0.0),
        };
        assert!(pad.activate(&mut body));
        assert_eq!(body.velocity, V(4.0, 12.0, 0.0));
        let mut pickup =
            TimedPickup::new("mega", PickupKind::Health, body.position, 100, 30).unwrap();
        assert_eq!(pickup.collect(body.position), Some(100));
        assert!(!pickup.available());
        for _ in 0..30 {
            pickup.step();
        }
        assert!(pickup.available());
        let spec = ProjectileSpec {
            speed: 30.0,
            direct_damage: 100.0,
            splash_damage: 80.0,
            splash_radius: 4.0,
            lifetime_ticks: 60,
        };
        let mut projectile = Projectile::spawn(1, V::ZERO, V(1.0, 0.0, 0.0), spec).unwrap();
        projectile.step(0.1);
        assert_eq!(projectile.position, V(3.0, 0.0, 0.0));
        assert!((spec.splash_at(2.0) - 40.0).abs() < 0.001);
    }

    #[test]
    fn armor_frag_scoring_and_respawn_follow_fixed_ticks() {
        let mut game = FragMatch::new(FragMatchConfig {
            frag_limit: 1,
            time_limit_ticks: 600,
            respawn_ticks: 2,
        })
        .unwrap();
        game.join(1);
        game.join(2);
        game.players.get_mut(&2).unwrap().armor = 50;
        assert!(game.damage(1, 2, 200.0));
        assert_eq!(game.winner(), Some(1));
        assert!(game.step().is_empty());
        assert_eq!(game.step(), vec![2]);
        assert_eq!(game.players[&2].health, 100);
    }
}
