//! Rift Delver's fixed-step authority. Rendering and audio only observe events.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::{
    controller::{Collider, Controller, ControllerState, Movement},
    devkit::{path, Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK},
    profile::ControllerProfile,
};

pub const ARENA: f32 = 22.;
pub const PERKS: [(&str, &str); 12] = [
    ("Overcharge", "+25% weapon damage"),
    ("Quick Circuit", "+18% fire rate"),
    ("Cold Sink", "+30% heat cooling"),
    ("Phase Rounds", "Shots pierce one more target"),
    ("Arc Relay", "Hits arc to a nearby enemy"),
    ("Volatile Core", "Kills explode into nearby foes"),
    ("Blood Circuit", "Every kill restores 2 health"),
    ("Reinforced", "+25 max health; heal 25"),
    ("Slipstream", "Dash recharges 20% faster"),
    ("Cryo Rounds", "Hits slow enemies for 2 seconds"),
    ("Flux Capacitor", "Shockwave recharges 25% faster"),
    ("Lucky Strike", "+15% critical chance"),
];
pub const WEAPONS: [&str; 3] = ["PULSE CARBINE", "SHARD CASTER", "ARC LANCE"];
pub const BIOMES: [&str; 3] = ["THE SUNKEN ARCHIVE", "EMBER FOUNDRY", "GLACIAL VAULT"];
pub const WORKSHOP: [(&str, &str); 4] = [
    ("Hull", "+12 starting health"),
    ("Reactor", "+8% starting damage"),
    ("Recovery", "+3 healing between waves"),
    ("Insurance", "+5% salvage kept on defeat"),
];

#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub look: [f32; 2],
    pub fire: bool,
    pub dash: bool,
    pub jump: bool,
    pub pulse: bool,
    pub interact: bool,
    pub choose: u8,
    pub weapon: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Phase {
    Camp,
    Combat,
    Draft,
    Gate,
    Lost,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Seeker,
    Drone,
    Charger,
    Brute,
    Sentinel,
    Warden,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    pub id: u32,
    pub kind: Kind,
    pub pos: V,
    pub hp: f32,
    pub max_hp: f32,
    pub timer: u32,
    pub slow: u32,
    pub flash: u32,
    pub dir: V,
}
impl Enemy {
    pub fn radius(&self) -> f32 {
        match self.kind {
            Kind::Warden => 1.9,
            Kind::Brute => 1.,
            Kind::Drone => 0.65,
            _ => 0.8,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bolt {
    pub pos: V,
    pub vel: V,
    pub life: u32,
    pub damage: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drop {
    pub pos: V,
    pub value: u32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Meta {
    pub bank: u32,
    pub levels: [u32; 4],
    pub best: u32,
    pub runs: u32,
    pub kills: u32,
    pub bosses: u32,
    pub extracted: u32,
    pub contracts: u32,
}
#[derive(Clone, Debug)]
pub enum Event {
    Fired { weapon: usize },
    Shot { from: V, to: V },
    Hit { at: V, damage: f32, crit: bool },
    Kill { at: V },
    Hurt,
    Dash,
    Pulse { at: V },
    Pickup { at: V, value: u32 },
    Wave,
    Choice,
    Banked { amount: u32 },
    Lost { kept: u32 },
    Denied,
    Bought,
    Boss,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub tick: u64,
    pub seed: u64,
    pub player: ControllerState,
    pub rng: Rng,
    pub phase: Phase,
    pub depth: u32,
    pub wave: u32,
    pub enemies: Vec<Enemy>,
    pub bolts: Vec<Bolt>,
    pub drops: Vec<Drop>,
    pub meta: Meta,
    pub haul: u32,
    pub run_kills: u32,
    pub perks: [u32; 12],
    pub offers: [usize; 3],
    pub weapon: usize,
    pub health: f32,
    pub heat: f32,
    pub dash_cd: u32,
    pub pulse_cd: u32,
    pub invuln: u32,
    pub fire_cd: u32,
    pub spawn_cd: u32,
    pub remaining: u32,
    pub next_id: u32,
    pub combat_ticks: u64,
    pub overheat: bool,
}
pub struct Sim {
    pub s: State,
    pub player: Controller,
    pub walls: Vec<Collider>,
    events: Vec<Event>,
}
fn controller() -> Controller {
    let p = ControllerProfile { walk_speed: 7.4, sprint_speed: 7.4, jump_height: 1.4, ..Default::default() };
    let mut c = Controller::for_profile(p, V(0., 0., 15.), 0.).unwrap();
    c.set_push_drag(7.);
    c
}
pub fn obstacles(depth: u32) -> Vec<Collider> {
    let mut v = vec![Collider { min: V(-ARENA - 1., -2., -ARENA - 1.), max: V(ARENA + 1., 0., ARENA + 1.) }];
    for (x, z, hx, hz) in
        [(0., -ARENA, ARENA, 0.8), (0., ARENA, ARENA, 0.8), (-ARENA, 0., 0.8, ARENA), (ARENA, 0., 0.8, ARENA)]
    {
        v.push(Collider { min: V(x - hx, 0., z - hz), max: V(x + hx, 5., z + hz) });
    }
    let layouts = [
        vec![(-8., -5., 1.8, 1.8), (8., 5., 1.8, 1.8), (-7., 8., 2.5, 1.), (7., -8., 2.5, 1.)],
        vec![(-9., 0., 1.2, 4.), (9., 0., 1.2, 4.), (0., -9., 3., 1.2), (0., 9., 3., 1.2)],
        vec![(-8., -8., 2., 2.), (8., 8., 2., 2.), (-8., 8., 2., 2.), (8., -8., 2., 2.)],
    ];
    for (x, z, hx, hz) in &layouts[(depth.saturating_sub(1) % 3) as usize] {
        v.push(Collider { min: V(x - hx, 0., z - hz), max: V(x + hx, 3.4, z + hz) });
    }
    v
}
impl Sim {
    pub fn new(seed: u64) -> Self {
        let player = controller();
        let s = State {
            tick: 0,
            seed,
            player: player.kinematic_state(),
            rng: Rng::new(seed),
            phase: Phase::Camp,
            depth: 1,
            wave: 1,
            enemies: vec![],
            bolts: vec![],
            drops: vec![],
            meta: Meta::default(),
            haul: 0,
            run_kills: 0,
            perks: [0; 12],
            offers: [0, 1, 2],
            weapon: 0,
            health: 100.,
            heat: 0.,
            dash_cd: 0,
            pulse_cd: 0,
            invuln: 0,
            fire_cd: 0,
            spawn_cd: 0,
            remaining: 0,
            next_id: 1,
            combat_ticks: 0,
            overheat: false,
        };
        Self { s, player, walls: obstacles(1), events: vec![] }
    }
    pub fn max_health(&self) -> f32 {
        100. + 12. * self.s.meta.levels[0] as f32 + 25. * self.s.perks[7] as f32
    }
    pub fn dash_time(&self) -> u32 {
        (100. / (1. + 0.2 * self.s.perks[8] as f32)) as u32
    }
    pub fn pulse_time(&self) -> u32 {
        (420. / (1. + 0.25 * self.s.perks[10] as f32)) as u32
    }
    pub fn cost(&self, i: usize) -> u32 {
        45 + 35 * self.s.meta.levels[i] + 12 * self.s.meta.levels[i].pow(2)
    }
    pub fn unlocked(&self) -> usize {
        if self.s.meta.best >= 6 {
            3
        } else if self.s.meta.best >= 3 {
            2
        } else {
            1
        }
    }
    pub fn biome(&self) -> usize {
        (self.s.depth.saturating_sub(1) % 3) as usize
    }
    pub fn remaining(&self) -> usize {
        self.s.remaining as usize + self.s.enemies.len()
    }
    pub fn start_run(&mut self) {
        self.s.meta.runs += 1;
        self.s.depth = 1;
        self.s.wave = 1;
        self.s.haul = 0;
        self.s.run_kills = 0;
        self.s.perks = [0; 12];
        self.s.health = self.max_health();
        self.s.heat = 0.;
        self.s.fire_cd = 0;
        self.s.dash_cd = 0;
        self.s.pulse_cd = 0;
        self.s.invuln = 90;
        self.s.drops.clear();
        self.s.enemies.clear();
        self.s.bolts.clear();
        self.player = controller();
        self.begin_wave();
    }
    fn begin_wave(&mut self) {
        self.s.phase = Phase::Combat;
        self.s.remaining = 7 + self.s.depth * 2 + self.s.wave * 3;
        self.s.spawn_cd = 30;
        self.s.combat_ticks = 0;
        self.s.bolts.clear();
        self.walls = obstacles(self.s.depth);
        self.events.push(Event::Wave);
        if self.s.wave == 3 && self.s.depth.is_multiple_of(3) {
            let hp = 250. + self.s.depth as f32 * 45.;
            self.s.enemies.push(Enemy {
                id: self.s.next_id,
                kind: Kind::Warden,
                pos: V(0., 2., -15.),
                hp,
                max_hp: hp,
                timer: 150,
                slow: 0,
                flash: 0,
                dir: V::ZERO,
            });
            self.s.next_id += 1;
            self.events.push(Event::Boss);
        }
    }
    fn draft(&mut self) {
        self.s.phase = Phase::Draft;
        self.s.bolts.clear();
        let mut ids: Vec<usize> = (0..PERKS.len()).collect();
        self.s.rng.shuffle(&mut ids);
        self.s.offers.copy_from_slice(&ids[..3]);
        self.s.health = (self.s.health + 12. + 3. * self.s.meta.levels[2] as f32).min(self.max_health());
        self.s.heat = 0.;
        self.events.push(Event::Choice);
    }
    pub fn extract(&mut self) {
        let reward = self.s.haul + 15 * self.s.depth;
        self.s.meta.bank += reward;
        self.s.meta.extracted += reward;
        self.s.haul = 0;
        self.s.phase = Phase::Camp;
        self.events.push(Event::Banked { amount: reward });
    }
    fn hurt(&mut self, damage: f32) {
        if self.s.invuln > 0 {
            return;
        }
        self.s.health -= damage;
        self.s.invuln = 36;
        self.events.push(Event::Hurt);
        if self.s.health <= 0. {
            self.s.health = 0.;
            let pct = 0.25 + 0.05 * self.s.meta.levels[3] as f32;
            let kept = (self.s.haul as f32 * pct) as u32;
            self.s.meta.bank += kept;
            self.s.haul = 0;
            self.s.phase = Phase::Lost;
            self.events.push(Event::Lost { kept });
        }
    }
    fn spawn(&mut self) {
        let side = self.s.rng.below(4);
        let a = self.s.rng.range(-17., 17.);
        let mut pos = match side {
            0 => V(a, 1., -19.),
            1 => V(a, 1., 19.),
            2 => V(-19., 1., a),
            _ => V(19., 1., a),
        };
        if (pos - self.player.position).length() < 9. {
            pos.0 = -pos.0;
            pos.2 = -pos.2;
        }
        let n = self.s.rng.below((2 + self.s.depth).min(6) as usize);
        let kind = match n {
            0 => Kind::Seeker,
            1 => Kind::Drone,
            2 => Kind::Charger,
            3 => Kind::Brute,
            _ => Kind::Sentinel,
        };
        let base = match kind {
            Kind::Brute => 85.,
            Kind::Sentinel => 44.,
            Kind::Charger => 38.,
            Kind::Drone => 25.,
            _ => 28.,
        };
        if kind == Kind::Drone {
            pos.1 = 2.8;
        }
        let hp = base * (1. + 0.13 * (self.s.depth - 1) as f32);
        self.s.enemies.push(Enemy {
            id: self.s.next_id,
            kind,
            pos,
            hp,
            max_hp: hp,
            timer: 60 + self.s.rng.below(100) as u32,
            slow: 0,
            flash: 0,
            dir: V::ZERO,
        });
        self.s.next_id += 1;
        self.s.remaining -= 1;
    }
    fn damage_at(&mut self, index: usize, damage: f32, crit: bool) {
        if let Some(e) = self.s.enemies.get_mut(index) {
            e.hp -= damage;
            e.flash = 5;
            if self.s.perks[9] > 0 {
                e.slow = 120;
            }
            self.events.push(Event::Hit { at: e.pos, damage, crit });
        }
    }
    fn shoot(&mut self) {
        self.events.push(Event::Fired { weapon: self.s.weapon });
        self.s.fire_cd = ((match self.s.weapon {
            0 => 9.,
            1 => 34.,
            _ => 24.,
        }) / (1. + 0.18 * self.s.perks[1] as f32))
            .max(3.) as u32;
        self.s.heat += (match self.s.weapon {
            0 => 0.055,
            1 => 0.19,
            _ => 0.14,
        }) / (1. + 0.1 * self.s.perks[2] as f32);
        if self.s.heat >= 1. {
            self.s.overheat = true;
        }
        let origin = self.player.position;
        let base = self.player.direction();
        let mult = (1. + 0.25 * self.s.perks[0] as f32) * (1. + 0.08 * self.s.meta.levels[1] as f32);
        let pellets = if self.s.weapon == 1 { 7 } else { 1 };
        for _ in 0..pellets {
            let spread = if self.s.weapon == 1 { 0.07 } else { 0.003 };
            let dir = (base
                + V(
                    self.s.rng.range(-spread, spread),
                    self.s.rng.range(-spread, spread),
                    self.s.rng.range(-spread, spread),
                ))
            .norm();
            let wall = wall_distance(origin, dir, &self.walls).min(75.);
            let mut hits: Vec<(usize, f32)> = self
                .s
                .enemies
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    ray_sphere(origin, dir, e.pos, e.radius() + 0.12).filter(|t| *t < wall).map(|t| (i, t))
                })
                .collect();
            hits.sort_by(|a, b| a.1.total_cmp(&b.1));
            let pierce = 1 + self.s.perks[3] as usize + usize::from(self.s.weapon == 2) * 2;
            let end = hits.first().map_or(wall, |h| h.1);
            self.events.push(Event::Shot { from: origin, to: origin + dir * end });
            for (i, _) in hits.into_iter().take(pierce) {
                let crit = self.s.rng.chance((0.08 + 0.15 * self.s.perks[11] as f32).min(0.8));
                let damage = (match self.s.weapon {
                    0 => 18.,
                    1 => 8.,
                    _ => 45.,
                }) * mult
                    * if crit { 2. } else { 1. };
                self.damage_at(i, damage, crit);
                if self.s.perks[4] > 0 {
                    let at = self.s.enemies[i].pos;
                    let target = self
                        .s
                        .enemies
                        .iter()
                        .enumerate()
                        .filter(|(j, e)| *j != i && e.hp > 0. && (e.pos - at).length() < 6.)
                        .min_by(|a, b| (a.1.pos - at).length().total_cmp(&(b.1.pos - at).length()))
                        .map(|(j, _)| j);
                    if let Some(j) = target {
                        self.events.push(Event::Shot { from: at, to: self.s.enemies[j].pos });
                        self.damage_at(j, damage * 0.35 * self.s.perks[4].min(5) as f32, false);
                    }
                }
            }
        }
    }
    fn kills(&mut self) {
        // Bounded chain reaction; each enemy dies once, then its explosion may kill the next.
        while let Some(index) = self.s.enemies.iter().position(|e| e.hp <= 0.) {
            let e = self.s.enemies.remove(index);
            self.s.run_kills += 1;
            self.s.meta.kills += 1;
            let boss = e.kind == Kind::Warden;
            let value = if boss {
                65 + self.s.depth * 6
            } else if e.kind == Kind::Brute {
                7 + self.s.depth
            } else {
                3 + self.s.depth
            };
            self.s.drops.push(Drop { pos: e.pos, value });
            self.s.health = (self.s.health + 2. * self.s.perks[6] as f32).min(self.max_health());
            self.events.push(Event::Kill { at: e.pos });
            if boss {
                self.s.meta.bosses += 1;
            }
            if self.s.perks[5] > 0 {
                let targets: Vec<usize> = self
                    .s
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| (t.pos - e.pos).length() < 4.5)
                    .map(|(i, _)| i)
                    .collect();
                for i in targets {
                    self.damage_at(i, 22. * self.s.perks[5] as f32, false);
                }
                self.events.push(Event::Pulse { at: e.pos });
            }
            let contracts = self.s.meta.kills / 100;
            if contracts > self.s.meta.contracts {
                self.s.meta.bank += 50 * (contracts - self.s.meta.contracts);
                self.s.meta.contracts = contracts;
                self.events.push(Event::Banked { amount: 50 });
            }
        }
    }
    pub fn step(&mut self, input: &Input) {
        self.s.tick += 1;
        if input.weapon && self.s.phase != Phase::Draft {
            self.s.weapon = (self.s.weapon + 1) % self.unlocked();
        }
        match self.s.phase {
            Phase::Camp => {
                if (1..=4).contains(&input.choose) {
                    let i = input.choose as usize - 1;
                    let cost = self.cost(i);
                    if self.s.meta.bank >= cost && self.s.meta.levels[i] < 10 {
                        self.s.meta.bank -= cost;
                        self.s.meta.levels[i] += 1;
                        self.events.push(Event::Bought);
                    } else {
                        self.events.push(Event::Denied);
                    }
                }
                if input.interact {
                    self.start_run();
                }
                self.s.player = self.player.kinematic_state();
                return;
            }
            Phase::Lost => {
                if input.interact {
                    self.s.phase = Phase::Camp;
                }
                return;
            }
            Phase::Draft => {
                if (1..=3).contains(&input.choose) {
                    let p = self.s.offers[input.choose as usize - 1];
                    self.s.perks[p] += 1;
                    if p == 7 {
                        self.s.health += 25.;
                    }
                    if self.s.wave == 3 {
                        self.s.phase = Phase::Gate;
                        self.s.meta.best = self.s.meta.best.max(self.s.depth);
                    } else {
                        self.s.wave += 1;
                        self.begin_wave();
                    }
                }
                return;
            }
            Phase::Gate => {
                if input.choose == 1 {
                    self.extract();
                } else if input.interact || input.choose == 2 {
                    self.s.depth += 1;
                    self.s.wave = 1;
                    self.player = controller();
                    self.s.drops.clear();
                    self.s.invuln = 90;
                    self.begin_wave();
                }
                self.s.player = self.player.kinematic_state();
                return;
            }
            Phase::Combat => {}
        }
        self.s.combat_ticks += 1;
        self.player.look(input.look[0], input.look[1], 1., false);
        self.s.dash_cd = self.s.dash_cd.saturating_sub(1);
        self.s.pulse_cd = self.s.pulse_cd.saturating_sub(1);
        self.s.invuln = self.s.invuln.saturating_sub(1);
        self.s.fire_cd = self.s.fire_cd.saturating_sub(1);
        if input.dash && self.s.dash_cd == 0 {
            let forward = path::forward(self.player.yaw);
            let right = path::right(self.player.yaw);
            let dir = if input.forward.abs() + input.right.abs() > 0.1 {
                (forward * input.forward + right * input.right).norm()
            } else {
                forward
            };
            self.player.apply_impulse(dir * 23.);
            self.s.invuln = 20;
            self.s.dash_cd = self.dash_time();
            self.events.push(Event::Dash);
        }
        self.player.update(
            Movement {
                forward: input.forward.clamp(-1., 1.),
                right: input.right.clamp(-1., 1.),
                jump: input.jump,
                ..Default::default()
            },
            TICK,
            &self.walls,
        );
        self.s.heat = (self.s.heat - 0.0032 * (1. + 0.3 * self.s.perks[2] as f32)).max(0.);
        if self.s.heat < 0.25 {
            self.s.overheat = false;
        }
        if input.fire && self.s.fire_cd == 0 && !self.s.overheat {
            self.shoot();
        }
        if input.pulse && self.s.pulse_cd == 0 {
            let at = self.player.position;
            self.s.pulse_cd = self.pulse_time();
            self.events.push(Event::Pulse { at });
            let targets: Vec<usize> =
                self.s.enemies.iter().enumerate().filter(|(_, e)| (e.pos - at).length() < 9.).map(|(i, _)| i).collect();
            for i in targets {
                self.damage_at(i, 50. * (1. + 0.2 * self.s.perks[10] as f32), false);
                self.s.enemies[i].slow = 150;
            }
            let p = at;
            self.s.bolts.retain(|b| (b.pos - p).length() > 10.);
        }
        self.kills();
        self.s.spawn_cd = self.s.spawn_cd.saturating_sub(1);
        if self.s.remaining > 0 && self.s.spawn_cd == 0 && self.s.enemies.len() < 28 {
            self.spawn();
            self.s.spawn_cd = (65u32.saturating_sub(self.s.depth * 3)).max(18);
        }
        let player = self.player.position;
        let mut new_bolts = vec![];
        let mut damage: f32 = 0.;
        for e in &mut self.s.enemies {
            e.flash = e.flash.saturating_sub(1);
            e.slow = e.slow.saturating_sub(1);
            e.timer = e.timer.saturating_sub(1);
            let delta = player - e.pos;
            let flat = V(delta.0, 0., delta.2);
            let distance = flat.length();
            let dir = flat.norm();
            let slow = if e.slow > 0 { 0.45 } else { 1. };
            let speed = match e.kind {
                Kind::Seeker => 3.8,
                Kind::Charger => 3.4,
                Kind::Brute => 2.3,
                Kind::Drone => 3.1,
                Kind::Sentinel => 1.8,
                Kind::Warden => 1.6,
            } * (1. + (self.s.depth as f32 * 0.025).min(0.6));
            let mut motion = dir * speed * slow * TICK;
            match e.kind {
                Kind::Drone | Kind::Sentinel => {
                    if distance < 10. {
                        motion = dir * -speed * 0.5 * TICK;
                    }
                    if e.timer == 0 && wall_distance(e.pos, delta.norm(), &self.walls) > delta.length() - 0.5 {
                        new_bolts.push(Bolt {
                            pos: e.pos,
                            vel: delta.norm() * 10.,
                            life: 240,
                            damage: 10. + self.s.depth as f32,
                        });
                        e.timer = if e.kind == Kind::Drone { 150 } else { 100 };
                    }
                }
                Kind::Charger => {
                    if e.timer == 42 {
                        e.dir = dir;
                    }
                    if e.timer > 0 && e.timer < 42 {
                        motion = if e.timer < 18 { e.dir * 18. * TICK } else { V::ZERO };
                    }
                    if e.timer == 0 {
                        e.timer = 180;
                    }
                }
                Kind::Warden => {
                    if distance < 8. {
                        motion = V::ZERO;
                    }
                    if e.timer == 0 {
                        for n in 0..12 {
                            let a = n as f32 * std::f32::consts::TAU / 12. + self.s.combat_ticks as f32 * 0.01;
                            new_bolts.push(Bolt {
                                pos: e.pos,
                                vel: V(a.sin() * 9., -0.15, a.cos() * 9.),
                                life: 300,
                                damage: 18.,
                            });
                        }
                        new_bolts.push(Bolt { pos: e.pos, vel: delta.norm() * 14., life: 180, damage: 22. });
                        e.timer = 90;
                    }
                }
                _ => {}
            }
            let r = e.radius() * 0.7;
            let mut at = e.pos + motion;
            let blockers = &self.walls[1..];
            if blockers.iter().any(|w| w.overlaps_xz(at, r)) {
                at = V(e.pos.0 + motion.0, e.pos.1, e.pos.2);
                if blockers.iter().any(|w| w.overlaps_xz(at, r)) {
                    at = V(e.pos.0, e.pos.1, e.pos.2 + motion.2);
                }
                if blockers.iter().any(|w| w.overlaps_xz(at, r)) {
                    // Tangential detour around a pillar; no teleporting through collision.
                    at = e.pos + V(-dir.2, 0., dir.0) * speed * TICK;
                    if blockers.iter().any(|w| w.overlaps_xz(at, r)) {
                        at = e.pos;
                    }
                }
            }
            e.pos = at;
            if e.kind == Kind::Drone {
                e.pos.1 = 2.6 + (self.s.combat_ticks as f32 * 0.03 + e.id as f32).sin() * 0.35;
            }
            if distance < e.radius() + 0.65 && (player.1 - e.pos.1).abs() < 2.2 {
                damage = damage.max(if e.kind == Kind::Brute || e.kind == Kind::Warden { 24. } else { 12. });
            }
        }
        self.s.bolts.extend(new_bolts);
        for b in &mut self.s.bolts {
            let next = b.pos + b.vel * TICK;
            b.life = b.life.saturating_sub(1);
            if self.walls[1..].iter().any(|w| w.contains(next)) {
                b.life = 0;
            }
            if (next - player).length() < 0.8 {
                damage = damage.max(b.damage);
                b.life = 0;
            }
            b.pos = next;
        }
        self.s.bolts.retain(|b| b.life > 0);
        if damage > 0. {
            self.hurt(damage);
        }
        if self.s.phase == Phase::Lost {
            self.s.player = self.player.kinematic_state();
            return;
        }
        let mut collected = vec![];
        for d in &mut self.s.drops {
            d.pos.1 += (0.65 - d.pos.1) * 0.08;
            let delta = V(player.0 - d.pos.0, 0., player.2 - d.pos.2);
            if delta.length() < 6. {
                d.pos = d.pos + delta.norm() * 10. * TICK;
            }
            if delta.length() < 1.1 {
                collected.push((d.pos, d.value));
                d.value = 0;
            }
        }
        self.s.drops.retain(|d| d.value > 0);
        for (at, value) in collected {
            self.s.haul += value;
            self.events.push(Event::Pickup { at, value });
        }
        if self.s.phase == Phase::Combat && self.remaining() == 0 {
            // Sweep remaining rewards into the haul so wave transitions never delete earned loot.
            self.s.haul += self.s.drops.iter().map(|d| d.value).sum::<u32>();
            self.s.drops.clear();
            self.draft();
        }
        self.s.player = self.player.kinematic_state();
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
pub fn ray_sphere(origin: V, dir: V, center: V, r: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - r * r;
    let d = b * b - c;
    if d < 0. {
        return None;
    }
    let t = -b - d.sqrt();
    if t >= 0. {
        Some(t)
    } else if c <= 0. {
        Some(0.)
    } else {
        None
    }
}
pub fn wall_distance(origin: V, dir: V, walls: &[Collider]) -> f32 {
    let mut nearest: f32 = 1000.;
    for w in walls {
        let mut lo: f32 = 0.;
        let mut hi: f32 = 1000.;
        for (o, d, a, b) in [
            (origin.0, dir.0, w.min.0, w.max.0),
            (origin.1, dir.1, w.min.1, w.max.1),
            (origin.2, dir.2, w.min.2, w.max.2),
        ] {
            if d.abs() < 1e-6 {
                if o < a || o > b {
                    hi = -1.;
                    break;
                }
            } else {
                let t1 = (a - o) / d;
                let t2 = (b - o) / d;
                lo = lo.max(t1.min(t2));
                hi = hi.min(t1.max(t2));
            }
        }
        if hi >= lo && hi >= 0. {
            nearest = nearest.min(lo.max(0.));
        }
    }
    nearest
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input)
    }
    fn state_hash(&self) -> u64 {
        let mut state = self.s.clone();
        state.player = self.player.kinematic_state();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut h = StateHasher::new();
        for b in bytes {
            h.u32(b as u32);
        }
        h.finish()
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "rift-delver";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = State;
    fn capture(&self) -> State {
        let mut s = self.s.clone();
        s.player = self.player.kinematic_state();
        s
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let player = &s.player;
        if !player.position.finite()
            || !player.velocity.finite()
            || !player.push.finite()
            || ![player.yaw, player.pitch, player.feet, player.body_height, player.vertical_velocity, s.health, s.heat]
                .iter()
                .all(|x| x.is_finite())
            || s.health < 0.
            || !(0.0..=1.5).contains(&s.heat)
            || !(0.1..=3.).contains(&player.body_height)
            || s.depth == 0
            || s.depth > 10000
            || !(1..=3).contains(&s.wave)
            || s.weapon >= 3
            || s.enemies.len() > 100
            || s.bolts.len() > 1000
            || s.drops.len() > 1000
            || s.offers.iter().any(|p| *p >= 12)
            || s.meta.levels.iter().any(|n| *n > 10)
            || s.perks.iter().any(|n| *n > 10000)
            || s.enemies.iter().any(|e| {
                !e.hp.is_finite() || !e.max_hp.is_finite() || e.max_hp <= 0. || !e.pos.finite() || !e.dir.finite()
            })
            || s.bolts.iter().any(|b| !b.pos.finite() || !b.vel.finite() || !b.damage.is_finite() || b.damage < 0.)
            || s.drops.iter().any(|d| !d.pos.finite())
        {
            return Err("Invalid expedition state".into());
        }
        self.player = controller();
        self.player.restore_kinematic_state(&s.player);
        self.walls = obstacles(s.depth);
        self.s = s;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.s.tick
    }
}

/// A player-like bot used for reproducible endurance and visual captures. It uses ordinary intentions.
pub fn autopilot(sim: &Sim) -> Input {
    match sim.s.phase {
        Phase::Camp | Phase::Lost => Input { interact: true, ..Default::default() },
        Phase::Gate => Input { choose: 2, ..Default::default() },
        Phase::Draft => {
            let priority = [0, 4, 6, 7, 1, 5, 2, 9, 3, 10, 8, 11];
            let chosen = sim
                .s
                .offers
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| priority.iter().position(|n| n == *p).unwrap())
                .map(|(i, _)| i + 1)
                .unwrap();
            Input { choose: chosen as u8, ..Default::default() }
        }
        Phase::Combat => {
            let p = sim.player.position;
            let target = sim
                .s
                .enemies
                .iter()
                .filter(|e| wall_distance(p, (e.pos - p).norm(), &sim.walls) > (e.pos - p).length() - e.radius())
                .min_by(|a, b| (a.pos - p).length().total_cmp(&(b.pos - p).length()))
                .or_else(|| sim.s.enemies.first());
            if let Some(e) = target {
                let d = e.pos - p;
                let yaw = path::yaw_of(d);
                let pitch = (d.1 / d.length().max(0.001)).clamp(-1., 1.).asin();
                let distance = V(d.0, 0., d.2).length();
                Input {
                    forward: if distance > 13. {
                        0.6
                    } else if distance < 8. {
                        -0.8
                    } else {
                        0.
                    },
                    right: 0.85,
                    look: [
                        path::wrap_angle(yaw - sim.player.yaw).clamp(-0.12, 0.12),
                        (sim.player.pitch - pitch).clamp(-0.08, 0.08),
                    ],
                    fire: true,
                    dash: distance < 5. && sim.s.dash_cd == 0,
                    pulse: sim.s.enemies.iter().filter(|e| (e.pos - p).length() < 8.).count() >= 3
                        && sim.s.pulse_cd == 0,
                    ..Default::default()
                }
            } else {
                Input { right: 0.5, look: [0.015, 0.], ..Default::default() }
            }
        }
    }
}
