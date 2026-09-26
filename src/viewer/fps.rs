//! Reusable, rendering-independent foundations for authoritative FPS games.
//!
//! The stock [`armory`] provides ten balanced prototype firearms. Games own the
//! presentation, networking shell and map, while this module owns validation,
//! deterministic firing/reload timing, aim-down-sights smoothing and team deathmatch
//! state. All time-sensitive behavior advances in fixed simulation ticks.

use crate::math::V;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FPS_TICK_RATE: u32 = 60;
pub const MAX_WEAPONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponClass {
    Pistol,
    SubmachineGun,
    AssaultRifle,
    Shotgun,
    MarksmanRifle,
    SniperRifle,
    MachineGun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireMode {
    SemiAutomatic,
    Automatic,
    Burst { rounds: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponDefinition {
    pub id: String,
    pub name: String,
    pub class: WeaponClass,
    pub fire_mode: FireMode,
    pub damage: f32,
    pub headshot_multiplier: f32,
    pub range: f32,
    pub rounds_per_minute: u16,
    pub magazine_size: u16,
    pub reserve_ammo: u16,
    pub reload_seconds: f32,
    pub pellets: u8,
    pub hip_spread_degrees: f32,
    pub ads_spread_degrees: f32,
    pub recoil_pitch_degrees: f32,
    pub recoil_yaw_degrees: f32,
    pub ads_fov_degrees: f32,
    pub ads_seconds: f32,
}

impl WeaponDefinition {
    pub fn validate(&self) -> crate::Result<()> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 48
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok || self.name.is_empty() || self.name.len() > 64 {
            return Err(format!("{}: invalid weapon id or name", self.id).into());
        }
        if !(1.0..=250.0).contains(&self.damage)
            || !(1.0..=5.0).contains(&self.headshot_multiplier)
            || !(1.0..=500.0).contains(&self.range)
            || !(30..=1500).contains(&self.rounds_per_minute)
            || !(1..=250).contains(&self.magazine_size)
            || !(0.1..=15.0).contains(&self.reload_seconds)
            || !(1..=32).contains(&self.pellets)
            || !(0.0..=20.0).contains(&self.ads_spread_degrees)
            || self.hip_spread_degrees < self.ads_spread_degrees
            || self.hip_spread_degrees > 30.0
            || !(20.0..=100.0).contains(&self.ads_fov_degrees)
            || !(0.03..=2.0).contains(&self.ads_seconds)
            || ![
                self.damage,
                self.headshot_multiplier,
                self.range,
                self.reload_seconds,
                self.hip_spread_degrees,
                self.ads_spread_degrees,
                self.recoil_pitch_degrees,
                self.recoil_yaw_degrees,
                self.ads_fov_degrees,
                self.ads_seconds,
            ]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(format!("{}: weapon values are outside supported bounds", self.id).into());
        }
        if matches!(self.fire_mode, FireMode::Burst { rounds: 0 | 1 }) {
            return Err(format!("{}: burst fire requires at least two rounds", self.id).into());
        }
        Ok(())
    }

    pub fn ticks_between_shots(&self) -> u32 {
        ((FPS_TICK_RATE as f32 * 60.0 / self.rounds_per_minute as f32).ceil() as u32).max(1)
    }

    pub fn reload_ticks(&self) -> u32 {
        (self.reload_seconds * FPS_TICK_RATE as f32).ceil() as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponCatalog {
    pub weapons: Vec<WeaponDefinition>,
}

impl WeaponCatalog {
    pub fn validate(&self) -> crate::Result<()> {
        if self.weapons.is_empty() || self.weapons.len() > MAX_WEAPONS {
            return Err(format!("weapon catalog must contain 1..={MAX_WEAPONS} entries").into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for weapon in &self.weapons {
            weapon.validate()?;
            if !ids.insert(&weapon.id) {
                return Err(format!("duplicate weapon id: {}", weapon.id).into());
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&WeaponDefinition> {
        self.weapons.iter().find(|weapon| weapon.id == id)
    }
}

#[allow(clippy::too_many_arguments)] // Compact, reviewable rows in the stock armory table.
fn weapon(
    id: &str,
    name: &str,
    class: WeaponClass,
    fire_mode: FireMode,
    damage: f32,
    range: f32,
    rpm: u16,
    magazine: u16,
    reserve: u16,
    reload: f32,
    pellets: u8,
    hip: f32,
    ads: f32,
    fov: f32,
) -> WeaponDefinition {
    WeaponDefinition {
        id: id.into(),
        name: name.into(),
        class,
        fire_mode,
        damage,
        headshot_multiplier: 1.65,
        range,
        rounds_per_minute: rpm,
        magazine_size: magazine,
        reserve_ammo: reserve,
        reload_seconds: reload,
        pellets,
        hip_spread_degrees: hip,
        ads_spread_degrees: ads,
        recoil_pitch_degrees: if pellets > 1 {
            3.8
        } else {
            1.1 + damage / 45.0
        },
        recoil_yaw_degrees: if pellets > 1 {
            1.7
        } else {
            0.45 + damage / 120.0
        },
        ads_fov_degrees: fov,
        ads_seconds: match class {
            WeaponClass::Pistol | WeaponClass::SubmachineGun => 0.13,
            WeaponClass::SniperRifle | WeaponClass::MachineGun => 0.28,
            _ => 0.20,
        },
    }
}

/// Balanced prototype armory used by BlueDM and available to future games.
pub fn armory() -> WeaponCatalog {
    use FireMode::{Automatic as Auto, Burst, SemiAutomatic as Semi};
    use WeaponClass::*;
    WeaponCatalog {
        weapons: vec![
            weapon(
                "kestrel-9",
                "Kestrel 9",
                Pistol,
                Semi,
                28.,
                55.,
                400,
                15,
                60,
                1.45,
                1,
                2.2,
                0.35,
                58.,
            ),
            weapon(
                "warden-45",
                "Warden .45",
                Pistol,
                Semi,
                38.,
                48.,
                315,
                10,
                50,
                1.70,
                1,
                2.8,
                0.42,
                56.,
            ),
            weapon(
                "riptide",
                "Riptide SMG",
                SubmachineGun,
                Auto,
                20.,
                42.,
                900,
                32,
                128,
                1.85,
                1,
                4.2,
                0.85,
                64.,
            ),
            weapon(
                "viper-pdw",
                "Viper PDW",
                SubmachineGun,
                Burst { rounds: 3 },
                23.,
                48.,
                780,
                30,
                120,
                1.95,
                1,
                3.7,
                0.72,
                62.,
            ),
            weapon(
                "atlas-556",
                "Atlas 5.56",
                AssaultRifle,
                Auto,
                30.,
                85.,
                690,
                30,
                120,
                2.25,
                1,
                3.4,
                0.48,
                60.,
            ),
            weapon(
                "boreal-762",
                "Boreal 7.62",
                AssaultRifle,
                Auto,
                38.,
                92.,
                540,
                25,
                100,
                2.45,
                1,
                4.1,
                0.55,
                58.,
            ),
            weapon(
                "bulwark-12",
                "Bulwark 12",
                Shotgun,
                Semi,
                14.,
                24.,
                90,
                8,
                40,
                2.65,
                8,
                7.8,
                4.2,
                66.,
            ),
            weapon(
                "lancer-dmr",
                "Lancer DMR",
                MarksmanRifle,
                Semi,
                54.,
                130.,
                260,
                14,
                56,
                2.35,
                1,
                3.8,
                0.18,
                48.,
            ),
            weapon(
                "northstar",
                "Northstar",
                SniperRifle,
                Semi,
                88.,
                220.,
                52,
                5,
                25,
                3.10,
                1,
                6.0,
                0.05,
                30.,
            ),
            weapon(
                "foundry-lmg",
                "Foundry LMG",
                MachineGun,
                Auto,
                27.,
                90.,
                600,
                60,
                180,
                4.10,
                1,
                5.2,
                0.95,
                62.,
            ),
        ],
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotIntent {
    pub sequence: u32,
    pub pellet: u8,
    pub spread_degrees: f32,
    pub recoil_pitch_degrees: f32,
    pub recoil_yaw_degrees: f32,
}

impl ShotIntent {
    /// Deterministic shot direction. The seed is derived from the authoritative shot
    /// sequence and pellet index, so clients may reproduce cosmetic tracers exactly.
    pub fn direction(self, yaw: f32, pitch: f32) -> V {
        let seed = self.sequence.wrapping_mul(0x9e37_79b9) ^ (u32::from(self.pellet) * 0x85eb_ca6b);
        let a = random01(seed) * std::f32::consts::TAU;
        let radius = random01(seed ^ 0xc2b2_ae35).sqrt() * self.spread_degrees.to_radians();
        direction(yaw + a.cos() * radius, pitch + a.sin() * radius)
    }
}

fn random01(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32
}

pub fn direction(yaw: f32, pitch: f32) -> V {
    V(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
    .norm()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponState {
    pub magazine: u16,
    pub reserve: u16,
    pub cooldown_ticks: u32,
    pub reload_ticks: u32,
    pub shot_sequence: u32,
    burst_remaining: u8,
}

impl WeaponState {
    pub fn new(definition: &WeaponDefinition) -> Self {
        Self {
            magazine: definition.magazine_size,
            reserve: definition.reserve_ammo,
            cooldown_ticks: 0,
            reload_ticks: 0,
            shot_sequence: 0,
            burst_remaining: 0,
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload_ticks > 0
    }

    pub fn begin_reload(&mut self, definition: &WeaponDefinition) -> bool {
        if self.reload_ticks > 0 || self.magazine >= definition.magazine_size || self.reserve == 0 {
            return false;
        }
        self.reload_ticks = definition.reload_ticks();
        self.burst_remaining = 0;
        true
    }

    /// Advance one fixed tick and return every pellet emitted on this tick.
    pub fn tick(
        &mut self,
        definition: &WeaponDefinition,
        trigger_pressed: bool,
        trigger_held: bool,
        reload_pressed: bool,
        ads_amount: f32,
    ) -> Vec<ShotIntent> {
        self.cooldown_ticks = self.cooldown_ticks.saturating_sub(1);
        if self.reload_ticks > 0 {
            self.reload_ticks -= 1;
            if self.reload_ticks == 0 {
                let wanted = definition.magazine_size - self.magazine;
                let moved = wanted.min(self.reserve);
                self.magazine += moved;
                self.reserve -= moved;
            }
            return Vec::new();
        }
        if reload_pressed || self.magazine == 0 {
            self.begin_reload(definition);
            return Vec::new();
        }
        if let FireMode::Burst { rounds } = definition.fire_mode {
            if trigger_pressed && self.burst_remaining == 0 {
                self.burst_remaining = rounds;
            }
        }
        let wants_fire = match definition.fire_mode {
            FireMode::SemiAutomatic => trigger_pressed,
            FireMode::Automatic => trigger_held,
            FireMode::Burst { .. } => self.burst_remaining > 0,
        };
        if !wants_fire || self.cooldown_ticks > 0 || self.magazine == 0 {
            return Vec::new();
        }
        self.magazine -= 1;
        self.cooldown_ticks = definition.ticks_between_shots();
        self.shot_sequence = self.shot_sequence.wrapping_add(1);
        self.burst_remaining = self.burst_remaining.saturating_sub(1);
        let spread = definition.hip_spread_degrees
            + (definition.ads_spread_degrees - definition.hip_spread_degrees)
                * ads_amount.clamp(0.0, 1.0);
        (0..definition.pellets)
            .map(|pellet| ShotIntent {
                sequence: self.shot_sequence,
                pellet,
                spread_degrees: spread,
                recoil_pitch_degrees: definition.recoil_pitch_degrees,
                recoil_yaw_degrees: definition.recoil_yaw_degrees
                    * (random01(self.shot_sequence ^ 0xa511_e9b3) * 2.0 - 1.0),
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AimState {
    amount: f32,
}

impl AimState {
    pub fn amount(self) -> f32 {
        self.amount
    }

    /// Smooth, frame-rate-independent ADS transition with exact clamping at the ends.
    pub fn update(&mut self, aiming: bool, dt: f32, definition: &WeaponDefinition) -> f32 {
        if !dt.is_finite() || dt <= 0.0 {
            return self.amount;
        }
        let target = if aiming { 1.0 } else { 0.0 };
        let response = 1.0 - (-6.0 * dt / definition.ads_seconds).exp();
        self.amount += (target - self.amount) * response;
        if (target - self.amount).abs() < 0.0005 {
            self.amount = target;
        }
        self.amount
    }

    pub fn fov(self, hip_fov: f32, definition: &WeaponDefinition) -> f32 {
        hip_fov + (definition.ads_fov_degrees - hip_fov) * smoothstep(self.amount)
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Team {
    Azure,
    Crimson,
}

impl Team {
    pub fn index(self) -> usize {
        match self {
            Self::Azure => 0,
            Self::Crimson => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperativeModel {
    Vanguard,
    Recon,
    Breacher,
    FieldTech,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combatant {
    pub team: Team,
    pub model: OperativeModel,
    pub health: u16,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub respawn_ticks: u32,
    pub spawn_protection_ticks: u32,
    pub last_attacker: Option<u64>,
}

impl Combatant {
    pub fn alive(&self) -> bool {
        self.health > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDeathmatchConfig {
    pub score_limit: u32,
    pub time_limit_ticks: u64,
    pub respawn_ticks: u32,
    pub spawn_protection_ticks: u32,
    pub friendly_fire: bool,
}

impl Default for TeamDeathmatchConfig {
    fn default() -> Self {
        Self {
            score_limit: 50,
            time_limit_ticks: 10 * 60 * FPS_TICK_RATE as u64,
            respawn_ticks: 3 * FPS_TICK_RATE,
            spawn_protection_ticks: FPS_TICK_RATE,
            friendly_fire: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageResult {
    pub applied: u16,
    pub killed: bool,
    pub score_awarded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RespawnEvent {
    pub player_id: u64,
    pub position: V,
    pub yaw: f32,
}

pub struct TeamDeathmatch {
    pub config: TeamDeathmatchConfig,
    pub tick: u64,
    pub team_scores: [u32; 2],
    pub players: BTreeMap<u64, Combatant>,
    azure_spawns: Vec<(V, f32)>,
    crimson_spawns: Vec<(V, f32)>,
    spawn_cursor: [usize; 2],
}

impl TeamDeathmatch {
    pub fn new(
        config: TeamDeathmatchConfig,
        azure_spawns: Vec<(V, f32)>,
        crimson_spawns: Vec<(V, f32)>,
    ) -> crate::Result<Self> {
        if config.score_limit == 0
            || config.time_limit_ticks == 0
            || azure_spawns.is_empty()
            || crimson_spawns.is_empty()
            || azure_spawns
                .iter()
                .chain(&crimson_spawns)
                .any(|(position, yaw)| !position.finite() || !yaw.is_finite())
        {
            return Err(
                "team deathmatch requires valid limits and at least one spawn per team".into(),
            );
        }
        Ok(Self {
            config,
            tick: 0,
            team_scores: [0, 0],
            players: BTreeMap::new(),
            azure_spawns,
            crimson_spawns,
            spawn_cursor: [0, 0],
        })
    }

    pub fn add_player(&mut self, player_id: u64, model: OperativeModel) -> Option<RespawnEvent> {
        if self.players.contains_key(&player_id) {
            return None;
        }
        let azure = self
            .players
            .values()
            .filter(|p| p.team == Team::Azure)
            .count();
        let crimson = self
            .players
            .values()
            .filter(|p| p.team == Team::Crimson)
            .count();
        let team = if azure <= crimson {
            Team::Azure
        } else {
            Team::Crimson
        };
        self.players.insert(
            player_id,
            Combatant {
                team,
                model,
                health: 100,
                kills: 0,
                deaths: 0,
                assists: 0,
                respawn_ticks: 0,
                spawn_protection_ticks: self.config.spawn_protection_ticks,
                last_attacker: None,
            },
        );
        let (position, yaw) = self.next_spawn(team);
        Some(RespawnEvent {
            player_id,
            position,
            yaw,
        })
    }

    pub fn remove_player(&mut self, player_id: u64) {
        self.players.remove(&player_id);
    }

    pub fn apply_damage(&mut self, attacker: u64, victim: u64, damage: f32) -> DamageResult {
        let Some(attacker_team) = self.players.get(&attacker).map(|p| p.team) else {
            return DamageResult {
                applied: 0,
                killed: false,
                score_awarded: false,
            };
        };
        let Some(target) = self.players.get_mut(&victim) else {
            return DamageResult {
                applied: 0,
                killed: false,
                score_awarded: false,
            };
        };
        if attacker == victim
            || !target.alive()
            || target.spawn_protection_ticks > 0
            || (!self.config.friendly_fire && target.team == attacker_team)
            || !damage.is_finite()
            || damage <= 0.0
        {
            return DamageResult {
                applied: 0,
                killed: false,
                score_awarded: false,
            };
        }
        let applied = damage.ceil().clamp(1.0, u16::MAX as f32) as u16;
        target.health = target.health.saturating_sub(applied);
        target.last_attacker = Some(attacker);
        let killed = target.health == 0;
        if killed {
            target.deaths += 1;
            target.respawn_ticks = self.config.respawn_ticks;
            if let Some(killer) = self.players.get_mut(&attacker) {
                killer.kills += 1;
            }
            self.team_scores[attacker_team.index()] += 1;
        }
        DamageResult {
            applied,
            killed,
            score_awarded: killed,
        }
    }

    pub fn step(&mut self) -> Vec<RespawnEvent> {
        self.tick += 1;
        let mut ready = Vec::new();
        for (&id, player) in &mut self.players {
            player.spawn_protection_ticks = player.spawn_protection_ticks.saturating_sub(1);
            if player.health == 0 {
                player.respawn_ticks = player.respawn_ticks.saturating_sub(1);
                if player.respawn_ticks == 0 {
                    player.health = 100;
                    player.spawn_protection_ticks = self.config.spawn_protection_ticks;
                    player.last_attacker = None;
                    ready.push((id, player.team));
                }
            }
        }
        ready
            .into_iter()
            .map(|(player_id, team)| {
                let (position, yaw) = self.next_spawn(team);
                RespawnEvent {
                    player_id,
                    position,
                    yaw,
                }
            })
            .collect()
    }

    pub fn winner(&self) -> Option<Team> {
        if self.team_scores[0] >= self.config.score_limit {
            Some(Team::Azure)
        } else if self.team_scores[1] >= self.config.score_limit {
            Some(Team::Crimson)
        } else if self.tick >= self.config.time_limit_ticks {
            match self.team_scores[0].cmp(&self.team_scores[1]) {
                std::cmp::Ordering::Greater => Some(Team::Azure),
                std::cmp::Ordering::Less => Some(Team::Crimson),
                std::cmp::Ordering::Equal => None,
            }
        } else {
            None
        }
    }

    fn next_spawn(&mut self, team: Team) -> (V, f32) {
        let index = team.index();
        let spawns = if team == Team::Azure {
            &self.azure_spawns
        } else {
            &self.crimson_spawns
        };
        let spawn = spawns[self.spawn_cursor[index] % spawns.len()];
        self.spawn_cursor[index] = self.spawn_cursor[index].wrapping_add(1);
        spawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_armory_has_ten_valid_distinct_firearms() {
        let catalog = armory();
        catalog.validate().unwrap();
        assert_eq!(catalog.weapons.len(), 10);
        assert_eq!(catalog.get("atlas-556").unwrap().magazine_size, 30);
        assert!(catalog.weapons.iter().any(|w| w.pellets > 1));
        assert!(catalog
            .weapons
            .iter()
            .any(|w| matches!(w.fire_mode, FireMode::Burst { .. })));
    }

    #[test]
    fn reload_and_fire_timing_are_fixed_tick_deterministic() {
        let def = armory().get("kestrel-9").unwrap().clone();
        let mut state = WeaponState::new(&def);
        assert_eq!(state.tick(&def, true, true, false, 0.0).len(), 1);
        assert_eq!(state.magazine, 14);
        for _ in 0..def.ticks_between_shots() - 1 {
            assert!(state.tick(&def, true, true, false, 0.0).is_empty());
        }
        assert_eq!(state.tick(&def, true, true, false, 0.0).len(), 1);
        state.magazine = 0;
        assert!(state.tick(&def, false, false, true, 0.0).is_empty());
        for _ in 0..def.reload_ticks() {
            state.tick(&def, false, false, false, 0.0);
        }
        assert_eq!(state.magazine, def.magazine_size);
    }

    #[test]
    fn ads_is_smooth_and_reduces_fov_and_spread() {
        let catalog = armory();
        let def = catalog.get("northstar").unwrap();
        let mut aim = AimState::default();
        let first = aim.update(true, 1.0 / 60.0, def);
        assert!(first > 0.0 && first < 1.0);
        for _ in 0..120 {
            aim.update(true, 1.0 / 60.0, def);
        }
        assert_eq!(aim.amount(), 1.0);
        assert!((aim.fov(75.0, def) - 30.0).abs() < 0.001);
    }

    #[test]
    fn tdm_balances_teams_rejects_friendly_fire_and_respawns() {
        let mut game = TeamDeathmatch::new(
            TeamDeathmatchConfig {
                respawn_ticks: 2,
                spawn_protection_ticks: 0,
                ..Default::default()
            },
            vec![(V(-4., 0., 0.), 1.57)],
            vec![(V(4., 0., 0.), -1.57)],
        )
        .unwrap();
        assert_eq!(
            game.add_player(10, OperativeModel::Vanguard)
                .unwrap()
                .position,
            V(-4., 0., 0.)
        );
        game.add_player(20, OperativeModel::Recon).unwrap();
        game.add_player(30, OperativeModel::Breacher).unwrap();
        assert_eq!(game.players[&10].team, Team::Azure);
        assert_eq!(game.players[&20].team, Team::Crimson);
        assert_eq!(game.apply_damage(10, 30, 200.).applied, 0);
        assert!(game.apply_damage(10, 20, 200.).killed);
        assert_eq!(game.team_scores, [1, 0]);
        assert!(game.step().is_empty());
        let events = game.step();
        assert_eq!(events.len(), 1);
        assert_eq!(game.players[&20].health, 100);
    }
}
