//! Rendering-free acceptance game. Native preview, headless walks and tests use this same authority.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vesper3d::math::V;
use vesper3d::viewer::{
    controller::{Collider, Controller, ControllerState, Movement},
    devkit::{SavePolicy, Simulation, Snapshot, StateHasher, TICK},
    narrative::{
        self, Command, Commands, Context, NarrativeState, Presentation, Registry, Runtime,
        SceneView, Value, ValueType, View,
    },
};

pub const CELL: V = V(-2.2, 1.15, 0.1);
pub const RELAY: V = V(0., 0., -0.6);
pub const FINISH: V = V(0., 0., -4.5);
pub const GATE: (V, V) = (V(-4., 0., -2.25), V(4., 2.3, -1.95));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fixture {
    Storyless,
    Reactive,
    Scene,
}
impl Fixture {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "storyless" => Ok(Self::Storyless),
            "reactive" => Ok(Self::Reactive),
            "scene" => Ok(Self::Scene),
            _ => Err("--fixture must be storyless, reactive or scene".into()),
        }
    }
    pub fn source(self) -> Option<PathBuf> {
        let file = match self {
            Self::Storyless => return None,
            Self::Reactive => "relay-yard.poe",
            Self::Scene => "borrowed-light.poe",
        };
        Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets/narrative")
                .join(file),
        )
    }
}

pub fn registry() -> Registry {
    Registry {
        queries: [
            ("inventory.cells", ValueType::Int),
            ("gate.open", ValueType::Bool),
            ("course.finished", ValueType::Bool),
            ("player.near_relay", ValueType::Bool),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect(),
        commands: [("bridge.power".into(), ValueType::Bool)]
            .into_iter()
            .collect(),
    }
}

/// Shared spatial geometry: the native client draws these exact collision boxes.
pub fn solids(gate_open: bool) -> Vec<Collider> {
    let mut boxes = vec![
        Collider {
            min: V(-4., -0.4, -6.),
            max: V(4., 0., 6.),
        },
        Collider {
            min: V(-4.3, 0., -6.),
            max: V(-4., 1.2, 6.),
        },
        Collider {
            min: V(4., 0., -6.),
            max: V(4.3, 1.2, 6.),
        },
        Collider {
            min: V(-4., 0., 5.8),
            max: V(4., 1.2, 6.),
        },
        Collider {
            min: V(-4., 0., -6.),
            max: V(4., 1.2, -5.8),
        },
        // Three 20 cm risers: the pickup can only be reached on the raised landing.
        Collider {
            min: V(-2.8, 0., 1.1),
            max: V(-1.6, 0.2, 1.8),
        },
        Collider {
            min: V(-2.8, 0., 0.5),
            max: V(-1.6, 0.4, 1.1),
        },
        Collider {
            min: V(-2.8, 0., -0.3),
            max: V(-1.6, 0.6, 0.5),
        },
    ];
    // Walkable relay plate and guide rails also share collision/render geometry.
    boxes.push(Collider {
        min: RELAY + V(-0.32, 0., -0.25),
        max: RELAY + V(0.32, 0.08, 0.25),
    });
    for x in [-1.3, 1.3] {
        boxes.push(Collider {
            min: V(x - 0.06, 0., -5.3),
            max: V(x + 0.06, 0.9, -2.7),
        });
    }
    if !gate_open {
        boxes.push(Collider {
            min: GATE.0,
            max: GATE.1,
        });
    }
    boxes
}

#[derive(Clone)]
pub struct World {
    pub tick: u64,
    pub seed: u64,
    pub player: Controller,
    pub cells: i64,
    pub cell_available: bool,
    pub gate_open: bool,
    pub finished: bool,
    pub power_commits: u32,
    pub speaker_present: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldState {
    pub tick: u64,
    pub seed: u64,
    pub player: ControllerState,
    pub cells: i64,
    pub cell_available: bool,
    pub gate_open: bool,
    pub finished: bool,
    pub power_commits: u32,
    pub speaker_present: bool,
}
impl World {
    pub fn new(seed: u64) -> Self {
        let mut player = Controller::for_profile(Default::default(), V(0., 0., 4.5), 0.)
            .expect("valid fixture spawn");
        player.set_floor(None);
        Self {
            tick: 0,
            seed,
            player,
            cells: 0,
            cell_available: true,
            gate_open: false,
            finished: false,
            power_commits: 0,
            speaker_present: true,
        }
    }
    pub fn feet(&self) -> V {
        V(
            self.player.position.0,
            self.player.feet_height(),
            self.player.position.2,
        )
    }
    pub fn near_relay(&self) -> bool {
        (self.feet() - RELAY).length() < 1.5
    }
    pub fn capture(&self) -> WorldState {
        WorldState {
            tick: self.tick,
            seed: self.seed,
            player: self.player.network_state(),
            cells: self.cells,
            cell_available: self.cell_available,
            gate_open: self.gate_open,
            finished: self.finished,
            power_commits: self.power_commits,
            speaker_present: self.speaker_present,
        }
    }
    fn restore(&mut self, s: WorldState) -> Result<(), String> {
        if !(0..=1).contains(&s.cells)
            || s.power_commits > 1
            || !s.player.position.finite()
            || !s.player.velocity.finite()
            || !s.player.yaw.is_finite()
            || !s.player.pitch.is_finite()
            || !s.player.feet.is_finite()
            || !s.player.body_height.is_finite()
            || !s.player.vertical_velocity.is_finite()
            || !s.player.push.finite()
        {
            return Err("invalid yard state".into());
        }
        self.tick = s.tick;
        self.seed = s.seed;
        self.player.restore_network_state(&s.player);
        self.cells = s.cells;
        self.cell_available = s.cell_available;
        self.gate_open = s.gate_open;
        self.finished = s.finished;
        self.power_commits = s.power_commits;
        self.speaker_present = s.speaker_present;
        Ok(())
    }
    pub fn state_hash(&self) -> u64 {
        // All controller internals influence future movement, including vertical velocity and pushes.
        let state = self.capture();
        let bytes = serde_json::to_vec(&state).expect("finite fixture state");
        StateHasher::new().bytes(&bytes).finish()
    }
    fn step(&mut self, input: &Input, in_scene: bool) -> Vec<&'static str> {
        self.tick += 1;
        let mut events = Vec::new();
        if !in_scene {
            self.player.look(input.look[0], input.look[1], 1., false);
            self.player.update(
                Movement {
                    forward: input.forward,
                    right: input.right,
                    jump: input.jump,
                    ..Default::default()
                },
                TICK,
                &solids(self.gate_open),
            );
            // The center-to-center distance includes height; drawing 3D over flat rules would fail this.
            let body = self.feet() + V(0., 0.55, 0.);
            if self.cell_available && (body - CELL).length() < 0.64 {
                self.cell_available = false;
                self.cells = 1;
                events.push("pickup");
            }
            if !self.finished && self.gate_open && (self.feet() - FINISH).length() < 1.0 {
                self.finished = true;
                events.push("finish");
            }
        }
        events
    }
}
impl View for World {
    fn query(&self, id: &str) -> Option<Value> {
        match id {
            "inventory.cells" => Some(Value::Int(self.cells)),
            "gate.open" => Some(Value::Bool(self.gate_open)),
            "course.finished" => Some(Value::Bool(self.finished)),
            "player.near_relay" => Some(Value::Bool(self.near_relay())),
            _ => None,
        }
    }
    fn speaker_present(&self, id: &str) -> bool {
        self.speaker_present && matches!(id, "pa" | "mara")
    }
}
impl Commands for World {
    /// Validate every consequence on a clone, then publish once. No partial reward or duplicate debit.
    fn apply_atomic(&mut self, commands: &[Command]) -> Result<(), String> {
        let mut proposed = self.clone();
        for command in commands {
            if command.id != "bridge.power" || command.value != Value::Bool(true) {
                return Err("unsupported yard command".into());
            }
            if proposed.cells != 1 || proposed.gate_open || !proposed.near_relay() {
                return Err("powering bridge requires one cell beside the closed relay".into());
            }
            proposed.cells -= 1;
            proposed.gate_open = true;
            proposed.power_commits += 1;
        }
        *self = proposed;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub look: [f32; 2],
    pub jump: bool,
    pub interact: bool,
    pub choice: Option<(u64, String)>,
    pub advance: Option<u64>,
    pub presented: Option<u64>,
    pub complete: Option<u64>,
    pub interrupt: Option<u64>,
    pub scene_interrupt: Option<u64>,
}

pub struct Sim {
    pub world: World,
    pub fixture: Fixture,
    pub narrative: Option<Runtime>,
    /// Diagnostics are transient presentation data, outside save state and gameplay hashes.
    pub notice: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SimState {
    pub fixture: Fixture,
    pub world: WorldState,
    pub narrative: Option<NarrativeState>,
}
impl Sim {
    pub fn new(fixture: Fixture, seed: u64) -> Result<Self, String> {
        Self::with_source(fixture, seed, fixture.source().as_deref())
    }
    pub fn with_source(fixture: Fixture, seed: u64, path: Option<&Path>) -> Result<Self, String> {
        let narrative = match path {
            None if fixture == Fixture::Storyless => None,
            None => return Err("selected narrative fixture needs content".into()),
            Some(_) if fixture == Fixture::Storyless => {
                return Err("storyless fixture accepts no narrative content".into())
            }
            Some(path) => {
                let content = narrative::load(path, &registry()).map_err(diagnostics)?;
                Some(Runtime::new(content, registry()).map_err(diagnostics)?)
            }
        };
        let mut sim = Self {
            world: World::new(seed),
            fixture,
            narrative,
            notice: None,
        };
        sim.event("arrive");
        Ok(sim)
    }
    pub fn event(&mut self, event: &str) {
        if let Some(runtime) = &mut self.narrative {
            let now = self.world.tick;
            // One context per authoritative tick/event; the queue itself is bounded by the runtime.
            let id = now * 8
                + match event {
                    "arrive" => 1,
                    "pickup" => 2,
                    "gate" => 3,
                    "finish" => 4,
                    _ => 7,
                };
            if let Err(error) = runtime.enqueue(Context {
                id,
                event: event.into(),
                expires_at: now + 300,
            }) {
                self.notice = Some(error);
            }
        }
    }
    pub fn scene(&self) -> Result<Option<SceneView>, String> {
        match &self.narrative {
            Some(r) => r.scene_view(&self.world),
            None => Ok(None),
        }
    }
    pub fn response(&self) -> Result<Option<Presentation>, String> {
        match &self.narrative {
            Some(r) => r.current_response(&self.world),
            None => Ok(None),
        }
    }
    pub fn reload(&mut self, path: &Path) -> Result<(), String> {
        let runtime = self
            .narrative
            .as_mut()
            .ok_or("storyless fixture has no content reload path")?;
        let content = narrative::load(path, &registry()).map_err(diagnostics)?;
        // Called between fixed ticks, with no command transaction in flight. Runtime checks active semantics.
        runtime.reload(content, &registry(), true)
    }
    pub fn step(&mut self, input: &Input) {
        self.notice = None;
        let in_scene = self
            .narrative
            .as_ref()
            .is_some_and(|r| r.state().scene.is_some());
        for event in self.world.step(input, in_scene) {
            self.event(event);
        }
        let mut gate_changed = false;
        if let Some(runtime) = &mut self.narrative {
            let result = (|| -> Result<(), String> {
                if let Some(token) = input.presented {
                    runtime.presented_response(token)?;
                }
                if let Some(token) = input.complete {
                    runtime.complete_response(token)?;
                }
                if let Some(token) = input.interrupt {
                    runtime.interrupt_response(token)?;
                }
                if let Some(token) = input.scene_interrupt {
                    runtime.interrupt_scene(token)?;
                }
                if let Some((token, id)) = &input.choice {
                    let before = self.world.gate_open;
                    runtime.choose(*token, id, &mut self.world)?;
                    gate_changed = !before && self.world.gate_open;
                }
                if let Some(token) = input.advance {
                    runtime.advance_scene(token, &self.world)?;
                }
                if input.interact && !in_scene && self.world.near_relay() {
                    if self.fixture == Fixture::Scene {
                        if self.world.cells != 1 || self.world.gate_open {
                            return Ok(());
                        }
                        if let Some(line) = runtime.current_response(&self.world)? {
                            runtime.interrupt_response(line.token)?;
                        }
                        runtime.start_scene("light.offer", &self.world)?;
                    } else if self.world.cells == 1 && !self.world.gate_open {
                        let command = Command {
                            id: "bridge.power".into(),
                            value: Value::Bool(true),
                            source: Default::default(),
                        };
                        self.world.apply_atomic(&[command])?;
                        gate_changed = true;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.notice = Some(error);
            }
        } else if input.interact
            && self.world.cells == 1
            && !self.world.gate_open
            && self.world.near_relay()
        {
            // The same ordinary gameplay verb remains usable with no narrative dependency or runtime.
            self.world.cells -= 1;
            self.world.gate_open = true;
            self.world.power_commits += 1;
        }
        if gate_changed {
            self.event("gate");
        }
        if let Some(runtime) = &mut self.narrative {
            if let Err(error) = runtime.dispatch(self.world.tick, &self.world) {
                self.notice = Some(error);
            }
        }
    }
}
fn diagnostics(errors: Vec<narrative::Diagnostic>) -> String {
    errors
        .into_iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
impl Simulation for Sim {
    type Input = Input;
    fn step(&mut self, input: &Input) {
        Sim::step(self, input)
    }
    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.world.state_hash()).u64(
            self.narrative
                .as_ref()
                .map_or(0, |r| r.state().state_hash()),
        );
        h.finish()
    }
    fn hash_parts(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("world", self.world.state_hash()),
            (
                "narrative",
                self.narrative
                    .as_ref()
                    .map_or(0, |r| r.state().state_hash()),
            ),
        ]
    }
}
impl Snapshot for Sim {
    const KIND: &'static str = "narrative-yard-fixture";
    const VERSION: u32 = 1;
    const POLICY: SavePolicy = SavePolicy::Exact;
    type State = SimState;
    fn capture(&self) -> SimState {
        SimState {
            fixture: self.fixture,
            world: self.world.capture(),
            narrative: self.narrative.as_ref().map(|r| r.state().clone()),
        }
    }
    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if state.fixture != self.fixture || state.narrative.is_some() != self.narrative.is_some() {
            return Err(
                "fixture capability changed; start a new run or provide an explicit save migration"
                    .into(),
            );
        }
        let mut proposed = self.world.clone();
        proposed.restore(state.world)?;
        if let (Some(runtime), Some(saved)) = (&mut self.narrative, state.narrative) {
            runtime.restore_state(saved)?;
        }
        self.world = proposed;
        self.notice = None;
        Ok(())
    }
    fn content(&self) -> Option<u64> {
        let mut h = StateHasher::new();
        h.bytes(&serde_json::to_vec(&solids(false)).expect("static geometry"))
            .f32(TICK)
            .f32(CELL.0)
            .f32(CELL.1)
            .f32(CELL.2)
            .u32(self.fixture as u32);
        if let Some(runtime) = &self.narrative {
            // Prose/localization edits are compatible; conditions, bindings and consequences are not.
            let mut content = runtime.content().clone();
            for response in &mut content.responses {
                response.text.clear();
                response.text_source = Default::default();
                response.source = Default::default();
                for condition in &mut response.conditions {
                    condition.source = Default::default();
                }
            }
            for scene in &mut content.scenes {
                scene.source = Default::default();
                for node in &mut scene.nodes {
                    node.text.clear();
                    node.text_source = Default::default();
                    node.source = Default::default();
                    for condition in &mut node.conditions {
                        condition.source = Default::default();
                    }
                    for choice in &mut node.choices {
                        choice.text.clear();
                        choice.source = Default::default();
                        for condition in &mut choice.conditions {
                            condition.source = Default::default();
                        }
                        for effect in &mut choice.effects {
                            effect.source = Default::default();
                        }
                    }
                }
            }
            h.bytes(&serde_json::to_vec(&content).expect("validated content"));
            h.bytes(&serde_json::to_vec(&registry()).expect("binding contract"));
        }
        Some(h.finish())
    }
    fn save_tick(&self) -> u64 {
        self.world.tick
    }
}

/// A bounded real-input walk. No teleport, copied inventory flag or mock command binding.
pub fn walk_inputs(tick: usize) -> Input {
    let mut input = Input::default();
    // Approach the landing from its south edge, climb its actual risers, then return to the relay.
    match tick {
        0..=40 => input.right = -1.,
        41..=119 => input.forward = 1.,
        120..=160 => input.right = 1.,
        161..=193 => input.forward = 1.,
        205..=305 => input.forward = 1.,
        _ => {}
    }
    input.interact = tick == 194;
    input
}
