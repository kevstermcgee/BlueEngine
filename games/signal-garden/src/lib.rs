//! Signal Garden rules. Fixed 60 Hz, integer state, no renderer or wall clock.
//! Six numbered relays need three sparks each. Carry at most three, hold Interact
//! beside a relay to charge it, and return to the beacon after the last relay.
pub mod audio;
use serde::{Deserialize, Serialize};
use vesper3d::viewer::devkit::{Migration, Rng, SavePolicy, Simulation, Snapshot, StateHasher};

pub const WIDTH: i32 = 17;
pub const HEIGHT: i32 = 13;
pub const LIMIT: u32 = 8 * 60 * 60;
pub const MOVE_TICKS: u32 = 12;
pub const CHARGE_TICKS: u32 = 90;
pub const BEACON: Cell = Cell(8, 6);
pub const RELAYS: [Cell; 6] = [Cell(2, 2), Cell(14, 2), Cell(14, 10), Cell(2, 10), Cell(8, 2), Cell(8, 10)];
pub const PADS: [Cell; 8] =
    [Cell(4, 3), Cell(12, 3), Cell(4, 9), Cell(12, 9), Cell(2, 6), Cell(14, 6), Cell(6, 6), Cell(10, 6)];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell(pub i32, pub i32);
impl Cell {
    pub fn distance(self, other: Self) -> i32 {
        (self.0 - other.0).abs() + (self.1 - other.1).abs()
    }
    pub fn walkable(self) -> bool {
        self.0 > 0
            && self.0 < WIDTH - 1
            && self.1 > 0
            && self.1 < HEIGHT - 1
            && ![(5, 4), (5, 5), (5, 7), (5, 8), (11, 4), (11, 5), (11, 7), (11, 8)].contains(&(self.0, self.1))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub dx: i32,
    pub dy: i32,
    pub interact: bool,
    pub shield: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Playing,
    Won,
    Lost,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Pickup(Cell),
    Charged(Cell),
    Relay(Cell),
    Hurt(Cell),
    Heal,
    Shield(Cell),
    Won,
    Lost,
}

/// One state record is shared by saves and the simulation: new rules cannot accidentally omit a
/// future-deciding field from a parallel save struct. Events are transient presentation data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub tick: u32,
    pub player: Cell,
    pub cargo: u32,
    pub hearts: u32,
    pub relay: usize,
    pub charges: [u32; 6],
    pub pads: [u32; 8],
    pub outcome: Outcome,
    pub shield_ticks: u32,
    move_wait: u32,
    charging: u32,
    hurt_wait: u32,
    heal_wait: u32,
}
pub struct Sim {
    pub state: State,
    events: Vec<Event>,
}
impl Sim {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            state: State {
                tick: 0,
                player: BEACON,
                cargo: 0,
                hearts: 5,
                relay: 0,
                charges: [0; 6],
                pads: std::array::from_fn(|_| rng.below(180) as u32),
                outcome: Outcome::Playing,
                shield_ticks: 0,
                move_wait: 0,
                charging: 0,
                hurt_wait: 0,
                heal_wait: 0,
            },
            events: Vec::new(),
        }
    }
    /// Patrol positions are pure functions of simulation time. The HUD shows their next tile.
    pub fn patrols(&self) -> [Cell; 3] {
        let t = (self.state.tick / 36) as i32;
        let bounce = |offset: i32, length: i32| {
            let n = (t + offset) % (2 * length);
            if n > length {
                2 * length - n
            } else {
                n
            }
        };
        [Cell(2 + bounce(0, 12), 4), Cell(14 - bounce(8, 12), 8), Cell(8, 2 + bounce(5, 8))]
    }
    pub fn step(&mut self, input: &Input) {
        let patrols = self.patrols();
        let s = &mut self.state;
        if s.outcome != Outcome::Playing {
            return;
        }
        s.tick += 1;
        s.shield_ticks = s.shield_ticks.saturating_sub(1);
        if input.shield && s.cargo > 0 && s.shield_ticks == 0 {
            s.cargo -= 1;
            s.shield_ticks = 180;
            self.events.push(Event::Shield(s.player));
        }
        s.move_wait = s.move_wait.saturating_sub(1);
        s.hurt_wait = s.hurt_wait.saturating_sub(1);
        s.heal_wait = s.heal_wait.saturating_sub(1);
        for cooldown in &mut s.pads {
            *cooldown = cooldown.saturating_sub(1);
        }
        let dx = input.dx.clamp(-1, 1);
        let dy = if dx == 0 { input.dy.clamp(-1, 1) } else { 0 };
        if s.move_wait == 0 && (dx != 0 || dy != 0) {
            let next = Cell(s.player.0 + dx, s.player.1 + dy);
            if next.walkable() {
                s.player = next;
                s.move_wait = MOVE_TICKS;
            }
        }
        if s.cargo < 3 {
            for (i, at) in PADS.iter().enumerate() {
                if *at == s.player && s.pads[i] == 0 {
                    s.cargo += 1;
                    s.pads[i] = 15 * 60;
                    self.events.push(Event::Pickup(*at));
                    break;
                }
            }
        }
        if s.player.distance(BEACON) > 1 && patrols.contains(&s.player) && s.hurt_wait == 0 && s.shield_ticks == 0 {
            s.hearts -= 1;
            s.hurt_wait = 120;
            s.cargo = s.cargo.saturating_sub(1);
            self.events.push(Event::Hurt(s.player));
        }
        if input.interact && s.player.distance(BEACON) <= 1 && s.hearts < 5 && s.heal_wait == 0 {
            s.hearts += 1;
            s.heal_wait = 30 * 60;
            self.events.push(Event::Heal);
        }
        if s.relay < RELAYS.len() && input.interact && s.cargo > 0 && s.player.distance(RELAYS[s.relay]) <= 1 {
            s.charging += 1;
            if s.charging == CHARGE_TICKS {
                s.charging = 0;
                s.cargo -= 1;
                s.charges[s.relay] += 1;
                self.events.push(Event::Charged(RELAYS[s.relay]));
                if s.charges[s.relay] == 3 {
                    self.events.push(Event::Relay(RELAYS[s.relay]));
                    s.relay += 1;
                }
            }
        } else {
            s.charging = 0;
        }
        if s.hearts == 0 || s.tick >= LIMIT {
            s.outcome = Outcome::Lost;
            self.events.push(Event::Lost);
        } else if s.relay == RELAYS.len() && s.player.distance(BEACON) <= 1 && input.interact {
            s.outcome = Outcome::Won;
            self.events.push(Event::Won);
        }
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    pub fn charging(&self) -> f32 {
        self.state.charging as f32 / CHARGE_TICKS as f32
    }
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        // Hash the complete authoritative record, including cooldowns and pad replenishment.
        StateHasher::new().bytes(&serde_json::to_vec(&self.state).expect("integer state serializes")).finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        vec![("garden", self.state_hash())]
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "signal-garden";
    const VERSION: u32 = 2;
    const MIGRATIONS: &'static [Migration] = &[Migration {
        from: 1,
        step: |mut state| {
            state
                .as_object_mut()
                .ok_or("version 1 garden state must be an object")?
                .insert("shield_ticks".into(), 0.into());
            Ok(state)
        },
    }];
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn save_tick(&self) -> u64 {
        self.state.tick as u64
    }
    fn restore(&mut self, s: State) -> Result<(), String> {
        let relays_valid = s.relay <= RELAYS.len()
            && s.charges.iter().enumerate().all(|(i, &n)| {
                if i < s.relay {
                    n == 3
                } else if i == s.relay {
                    n < 3
                } else {
                    n == 0
                }
            });
        let outcome_valid = match s.outcome {
            Outcome::Playing => s.tick < LIMIT && s.hearts > 0,
            Outcome::Won => s.relay == 6 && s.hearts > 0 && s.tick < LIMIT && s.player.distance(BEACON) <= 1,
            Outcome::Lost => s.hearts == 0 || s.tick == LIMIT,
        };
        if !s.player.walkable()
            || s.cargo > 3
            || s.shield_ticks > 180
            || s.hearts > 5
            || s.tick > LIMIT
            || !relays_valid
            || !outcome_valid
            || s.move_wait > MOVE_TICKS
            || s.charging >= CHARGE_TICKS
            || s.hurt_wait > 120
            || s.heal_wait > 1800
            || s.pads.iter().any(|&n| n > 900)
        {
            return Err(
                "invalid garden save: check position, inventory, relay order, outcome and cooldown bounds".into()
            );
        }
        self.state = s;
        self.events.clear();
        Ok(())
    }
}
