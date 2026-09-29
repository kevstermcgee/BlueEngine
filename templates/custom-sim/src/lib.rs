//! The game's rules: a pure, deterministic, fixed-step simulation with no window, no sound device and
//! no wall clock. `main.rs` presents it; tests and bots drive it directly.
//!
//! The starter is a tiny game to replace: collect glowing orbs on a platform floating in a void while
//! bumpers shove you around, and don't fall off. It shows the parts every action game needs: seeded
//! randomness, one `Input` per tick, engine `Controller` movement (with no implicit floor and knockback
//! impulses), events for the presentation, a state hash for determinism tests, and save states (F5 / F9).
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, SavePolicy, Simulation, Snapshot, StateHasher, TICK};

/// Half the platform's side length, in metres.
pub const PLATFORM_HALF: f32 = 6.;
/// Below this eye height the run is over.
pub const VOID_Y: f32 = -8.;
/// Orbs on the platform at any time.
pub const ORBS: usize = 3;
/// Bumper patrol speed in m/s.
pub const BUMPER_SPEED: f32 = 2.4;
/// Horizontal speed a bumper's shove gives the player, in m/s.
pub const KNOCKBACK: f32 = 7.5;

/// One tick of player intent. The window builds it from devices, tests and bots build it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    /// Forward (+1) and right (+1) movement, already in -1..=1.
    pub forward: f32,
    pub right: f32,
    /// Radian look deltas since the previous tick: `[yaw, pitch]`.
    pub look: [f32; 2],
    /// A press edge (true for exactly one tick per press).
    pub jump: bool,
}

/// What happened this tick; the window reacts with sound, particles and shake.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Collected { at: V, score: u32 },
    Bumped { at: V },
    Jumped,
    Landed,
    Fell,
}

/// A patrolling obstacle that shoves the player.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bumper {
    pub pos: V,
    pub vel: V,
}

/// The whole game state.
pub struct Sim {
    pub tick: u64,
    pub player: Controller,
    pub orbs: Vec<V>,
    pub bumpers: Vec<Bumper>,
    pub score: u32,
    pub over: bool,
    ground: Collider,
    rng: Rng,
    events: Vec<Event>,
    was_grounded: bool,
    bump_cooldown: u32,
}

impl Sim {
    /// A fresh game; the same seed always plays out the same way.
    pub fn new(seed: u64) -> Self {
        let mut player = Controller::for_profile(Default::default(), V(0., 0., PLATFORM_HALF - 1.5), 0.)
            .expect("the default profile is valid");
        // No implicit floor at y = 0: the platform's collider is the only ground, so its edge is a cliff.
        player.set_floor(None);
        let mut sim = Self {
            tick: 0,
            player,
            orbs: Vec::new(),
            bumpers: vec![
                Bumper { pos: V(-3., 0.4, -2.), vel: V(BUMPER_SPEED, 0., 0.) },
                Bumper { pos: V(3., 0.4, 1.), vel: V(0., 0., -BUMPER_SPEED) },
            ],
            score: 0,
            over: false,
            // The platform: its top is at y = 0 and it is one metre thick.
            ground: Collider { min: V(-PLATFORM_HALF, -1., -PLATFORM_HALF), max: V(PLATFORM_HALF, 0., PLATFORM_HALF) },
            rng: Rng::new(seed),
            events: Vec::new(),
            was_grounded: true,
            bump_cooldown: 0,
        };
        for _ in 0..ORBS {
            let orb = sim.random_orb();
            sim.orbs.push(orb);
        }
        sim
    }

    fn random_orb(&mut self) -> V {
        let r = PLATFORM_HALF - 1.;
        V(self.rng.range(-r, r), 0.6, self.rng.range(-r, r))
    }

    /// Advance exactly one 60 Hz tick. A finished run ignores further input.
    pub fn step(&mut self, input: &Input) {
        if self.over {
            return;
        }
        self.tick += 1;
        self.player.look(input.look[0], input.look[1], 1., false);
        let movement = Movement { forward: input.forward, right: input.right, jump: input.jump, ..Default::default() };
        self.player.update(movement, TICK, std::slice::from_ref(&self.ground));
        let grounded = self.player.is_grounded();
        if input.jump && self.was_grounded && !grounded {
            self.events.push(Event::Jumped);
        }
        if grounded && !self.was_grounded {
            self.events.push(Event::Landed);
        }
        self.was_grounded = grounded;

        let feet = V(self.player.position.0, self.player.feet_height(), self.player.position.2);
        // Orbs are collected by touching them.
        let center = V(feet.0, feet.1 + 0.6, feet.2);
        let (taken, kept): (Vec<V>, Vec<V>) = self.orbs.iter().partition(|orb| (**orb - center).length() < 0.8);
        self.orbs = kept;
        for at in taken {
            self.score += 1;
            self.events.push(Event::Collected { at, score: self.score });
            let orb = self.random_orb();
            self.orbs.push(orb);
        }
        // Bumpers patrol, bounce off the platform edge, and shove the player on contact.
        self.bump_cooldown = self.bump_cooldown.saturating_sub(1);
        let limit = PLATFORM_HALF - 0.5;
        for bumper in &mut self.bumpers {
            bumper.pos = bumper.pos + bumper.vel * TICK;
            if bumper.pos.0.abs() > limit {
                bumper.vel.0 = -bumper.vel.0;
                bumper.pos.0 = bumper.pos.0.clamp(-limit, limit);
            }
            if bumper.pos.2.abs() > limit {
                bumper.vel.2 = -bumper.vel.2;
                bumper.pos.2 = bumper.pos.2.clamp(-limit, limit);
            }
            let away = V(feet.0 - bumper.pos.0, 0., feet.2 - bumper.pos.2);
            if self.bump_cooldown == 0 && feet.1 < 0.9 && away.length() < 0.9 {
                let direction = if away.length() > 1e-3 { away.norm() } else { V(1., 0., 0.) };
                // A push plus a small hop: the engine's impulse channel, not a teleport.
                self.player.apply_impulse(direction * KNOCKBACK + V(0., 2.5, 0.));
                self.bump_cooldown = 30;
                self.events.push(Event::Bumped { at: bumper.pos });
            }
        }
        if self.player.position.1 < VOID_Y {
            self.over = true;
            self.events.push(Event::Fell);
        }
    }

    /// Events since the last call, oldest first.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input);
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.tick).u32(self.score).bool(self.over);
        let p = self.player.position;
        h.f32(p.0).f32(p.1).f32(p.2).f32(self.player.yaw).f32(self.player.pitch);
        for orb in &self.orbs {
            h.f32(orb.0).f32(orb.2);
        }
        for b in &self.bumpers {
            h.f32(b.pos.0).f32(b.pos.2).f32(b.vel.0).f32(b.vel.2);
        }
        h.finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        let one = |f: &dyn Fn(&mut StateHasher)| {
            let mut h = StateHasher::new();
            f(&mut h);
            h.finish()
        };
        vec![
            ("run", one(&|h| {
                h.u64(self.tick).u32(self.score).bool(self.over);
            })),
            ("player", one(&|h| {
                let p = self.player.position;
                h.f32(p.0).f32(p.1).f32(p.2).f32(self.player.yaw).f32(self.player.pitch);
            })),
            ("orbs", one(&|h| {
                for orb in &self.orbs {
                    h.f32(orb.0).f32(orb.2);
                }
            })),
            ("bumpers", one(&|h| {
                for b in &self.bumpers {
                    h.f32(b.pos.0).f32(b.pos.2).f32(b.vel.0).f32(b.vel.2);
                }
            })),
        ]
    }
}

/// Everything that decides where the game goes next, as plain data for a save file. Presentation (particles,
/// sounds, camera) is not in it: the window rebuilds those from the state after a load.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub player: ControllerState,
    pub orbs: Vec<V>,
    pub bumpers: Vec<Bumper>,
    pub score: u32,
    pub over: bool,
    pub rng: Rng,
    pub was_grounded: bool,
    pub bump_cooldown: u32,
}

/// Save states. Add every new field of `Sim` to `SimState` (bump `VERSION` and add a `Migration` when a field
/// changes shape); `tests/determinism.rs` fails at the first tick that reads something a save forgot.
///
/// `POLICY` is what a save promises, proved by that test (`assert_resumes_as_promised`). `Exact` is the bar
/// for rules, timers and random streams. If the game embeds a rigid-body world (`HeadlessWorld`, or
/// `vesper3d::rapier`), put its `save_state()` here and declare `SavePolicy::PhysicsContinuation`: contact
/// caches are history no save carries, so a resumed run is a pure function of the file but not bit-identical
/// (docs/SAVE_STATE.md, "Which contract a physics game can meet"); bound the drift with
/// `assert_resumes_within` if the game promises one. `hash_parts` names the pieces of the state hash so a
/// load that does not restore names the forgotten field.
impl Snapshot for Sim {
    const KIND: &'static str = "{{name}}";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            player: self.player.network_state(),
            orbs: self.orbs.clone(),
            bumpers: self.bumpers.clone(),
            score: self.score,
            over: self.over,
            rng: self.rng.clone(),
            was_grounded: self.was_grounded,
            bump_cooldown: self.bump_cooldown,
        }
    }
    /// Refuse a state this game could not have produced; the caller then keeps the running game.
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        let near = |p: V| p.0.abs() <= PLATFORM_HALF && p.2.abs() <= PLATFORM_HALF && p.1.abs() <= 100.;
        if state.orbs.len() != ORBS || !state.orbs.iter().all(|orb| near(*orb)) {
            return Err(format!("the save needs {ORBS} orbs on the platform"));
        }
        if state.bumpers.len() != self.bumpers.len() || !state.bumpers.iter().all(|b| near(b.pos)) {
            return Err("the save's bumpers do not match this game".into());
        }
        if state.player.position.1 < VOID_Y - 50. || state.player.position.1 > 1000. {
            return Err("the save puts the player outside the world".into());
        }
        self.tick = state.tick;
        self.player.restore_network_state(&state.player);
        self.orbs = state.orbs;
        self.bumpers = state.bumpers;
        self.score = state.score;
        self.over = state.over;
        self.rng = state.rng;
        self.was_grounded = state.was_grounded;
        self.bump_cooldown = state.bump_cooldown;
        self.events.clear();
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}
