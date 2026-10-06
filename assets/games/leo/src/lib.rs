//! Leo's deterministic, saveable walk through an endless temperate storybook world.
//! Engine movement/coordinates/saves; this game chooses plants and day length.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::{
    controller::{Collider, Controller, ControllerState, Movement},
    devkit::{
        procedural::{self, ChunkCache, ChunkId, DayCycle, DayTime, WorldPoint},
        SavePolicy, Simulation, Snapshot, StateHasher, TICK,
    },
    profile::ControllerProfile,
};
pub const CHUNK_SIZE: f32 = 32.;
pub const RADIUS: u8 = 3;
pub const DAY_TICKS: u32 = 60 * 12 * 60;
pub const PLANT_NAMES: [&str; 8] =
    ["Oak", "Silver birch", "Scots pine", "Rowan", "Daisy", "Buttercup", "Red clover", "Cornflower"];
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub sprint: bool,
    pub jump: bool,
    pub look: [f32; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub kind: usize,
    pub at: [f32; 2],
    pub scale: f32,
    pub yaw: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Meadow {
    pub plants: Vec<Plant>,
    pub forest: f32,
}
pub fn meadow(seed: u64, id: ChunkId) -> Result<Meadow, String> {
    let mut rng = id.rng(seed, 0x4c454f);
    let point = |at| WorldPoint::new(id, at, CHUNK_SIZE);
    let forest = procedural::field(seed, point([16.; 2])?, CHUNK_SIZE, 4)?;
    let mut plants = Vec::new();
    for z in 0..5 {
        for x in 0..5 {
            let at = [3.2 + x as f32 * 6.4 + rng.range(-1., 1.), 3.2 + z as f32 * 6.4 + rng.range(-1., 1.)];
            let density = ((procedural::field(seed, point(at)?, CHUNK_SIZE, 4)? - 0.3) * 1.7).clamp(0.03, 0.85);
            let clearing = id == ChunkId::default() && (at[0] - 16.).hypot(at[1] - 16.) < 11.;
            if !clearing && rng.chance(density) {
                plants.push(Plant {
                    kind: rng.below(4),
                    at,
                    scale: rng.range(0.7, 1.25),
                    yaw: rng.range(0., std::f32::consts::TAU),
                });
            }
        }
    }
    for i in 0..80 {
        plants.push(Plant {
            kind: if i < 32 { 4 + rng.below(4) } else { 8 },
            at: [rng.range(0.5, 31.5), rng.range(0.5, 31.5)],
            scale: rng.range(0.7, 1.4),
            yaw: rng.range(0., std::f32::consts::TAU),
        });
    }
    Ok(Meadow { plants, forest })
}
pub fn boy_profile() -> ControllerProfile {
    ControllerProfile {
        height: 1.35,
        crouched_height: 0.9,
        radius: 0.2,
        eye_height: 1.23,
        walk_speed: 3.6,
        sprint_speed: 6.,
        crouch_speed: 2.,
        jump_height: 0.45,
    }
}
pub struct Sim {
    pub tick: u64,
    pub seed: u64,
    pub origin: ChunkId,
    pub player: Controller,
    pub previous: V,
    pub chunks: ChunkCache<Meadow>,
    pub day_ticks: u32,
    colliders: Vec<Collider>,
}
impl Sim {
    pub fn new(seed: u64) -> Self {
        Self::with_day(seed, DAY_TICKS).expect("valid Leo defaults")
    }
    pub fn with_day(seed: u64, day_ticks: u32) -> Result<Self, String> {
        DayCycle::new(day_ticks)?;
        let player = Controller::for_profile(boy_profile(), V(16., 0., 16.), 0.).map_err(|e| e.to_string())?;
        let mut sim = Self {
            tick: 0,
            seed,
            origin: ChunkId::default(),
            previous: player.position,
            player,
            chunks: ChunkCache::new(RADIUS)?,
            day_ticks,
            colliders: Vec::new(),
        };
        sim.stream()?;
        Ok(sim)
    }
    fn stream(&mut self) -> Result<(), String> {
        let changes = self.chunks.update(self.origin, |id| meadow(self.seed, id))?;
        if changes.added.is_empty() && changes.removed.is_empty() {
            return Ok(());
        }
        self.colliders.clear();
        for (id, chunk) in self.chunks.chunks() {
            for p in chunk.plants.iter().filter(|p| p.kind < 4) {
                let [x, z] = WorldPoint { chunk: *id, local: p.at }.relative(self.origin, CHUNK_SIZE)?;
                let half = 0.22 * p.scale;
                self.colliders
                    .push(Collider { min: V(x - half, 0., z - half), max: V(x + half, 3. * p.scale, z + half) });
            }
        }
        Ok(())
    }
    pub fn time(&self) -> DayTime {
        DayCycle::new(self.day_ticks).unwrap().at(self.tick.saturating_add(u64::from(self.day_ticks) * 23 / 100))
    }
    pub fn point(&self) -> WorldPoint {
        WorldPoint { chunk: self.origin, local: [self.player.position.0, self.player.position.2] }
    }
    pub fn forest(&self) -> f32 {
        procedural::field(self.seed, self.point(), CHUNK_SIZE, 4).unwrap_or(0.5)
    }
    pub fn interpolated(&self, alpha: f32) -> V {
        self.previous.lerp(self.player.position, alpha.clamp(0., 1.))
    }
    pub fn camera_eye(&self, anchor: V, desired: V) -> Result<V, String> {
        vesper3d::viewer::camera::sweep_boom(anchor, desired, &self.colliders, 0.16).map_err(|e| e.to_string())
    }
    pub fn step(&mut self, input: &Input) -> Result<(), String> {
        self.previous = self.player.position;
        self.player.look(input.look[0], input.look[1], 1., false);
        self.player.update(
            Movement {
                forward: input.forward,
                right: input.right,
                sprint: input.sprint,
                jump: input.jump,
                ..Default::default()
            },
            TICK,
            &self.colliders,
        );
        let point = WorldPoint::new(self.origin, [self.player.position.0, self.player.position.2], CHUNK_SIZE)?;
        if point.chunk != self.origin {
            let shift = V(self.player.position.0 - point.local[0], 0., self.player.position.2 - point.local[1]);
            self.previous = self.previous - shift;
            self.player.position.0 = point.local[0];
            self.player.position.2 = point.local[1];
            self.origin = point.chunk;
            self.stream()?;
        }
        self.tick = self.tick.checked_add(1).ok_or("Leo's tick counter is exhausted")?;
        Ok(())
    }
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input).expect("valid simulation coordinates");
    }
    fn state_hash(&self) -> u64 {
        let s = self.capture();
        let mut h = StateHasher::new();
        h.u64(s.tick).u64(s.seed).u64(s.origin.x as u64).u64(s.origin.z as u64).u32(s.day_ticks);
        for byte in serde_json::to_vec(&s.player).unwrap() {
            h.u32(u32::from(byte));
        }
        h.finish()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimState {
    pub tick: u64,
    pub seed: u64,
    pub origin: ChunkId,
    pub player: ControllerState,
    pub day_ticks: u32,
}
impl Snapshot for Sim {
    const KIND: &'static str = "leo";
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            seed: self.seed,
            origin: self.origin,
            player: self.player.network_state(),
            day_ticks: self.day_ticks,
        }
    }
    fn restore(&mut self, s: SimState) -> Result<(), String> {
        DayCycle::new(s.day_ticks)?;
        WorldPoint { chunk: s.origin, local: [s.player.position.0, s.player.position.2] }
            .relative(s.origin, CHUNK_SIZE)?;
        let p = &s.player;
        if [
            p.position.1,
            p.yaw,
            p.pitch,
            p.velocity.0,
            p.velocity.1,
            p.velocity.2,
            p.feet,
            p.body_height,
            p.vertical_velocity,
            p.push.0,
            p.push.1,
            p.push.2,
        ]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1000.)
            || !(0. ..=10.).contains(&p.feet)
            || !(0.9..=1.35).contains(&p.body_height)
        {
            return Err("save has invalid boy movement state".into());
        }
        let mut next = Self::with_day(s.seed, s.day_ticks)?;
        next.origin = s.origin;
        next.player.restore_network_state(&s.player);
        next.previous = next.player.position;
        next.tick = s.tick;
        next.stream()?;
        *self = next;
        Ok(())
    }
    fn save_tick(&self) -> u64 {
        self.tick
    }
}

extern crate self as leo;
pub mod browser;
#[cfg(feature = "portable-client")]
pub mod scene;
