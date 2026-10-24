//! Small 2D facade: integer rules, logical-pixel presentation and shared runtime services.
pub use crate::runtime::{Simulation, Snapshot, StateHasher};
use serde::{Deserialize, Serialize};
#[cfg(feature = "two-d")]
pub mod client;
#[cfg(feature = "two-d-draw")]
pub mod draw;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }
    pub fn overlaps(self, b: Self) -> bool {
        self.w > 0
            && self.h > 0
            && b.w > 0
            && b.h > 0
            && self.x < b.x + b.w
            && self.x + self.w > b.x
            && self.y < b.y + b.h
            && self.y + self.h > b.y
    }
    /// Swept integer movement, one unit at a time; touching is not penetration. Returns blocked axes.
    pub fn slide(&mut self, dx: i32, dy: i32, walls: &[Self]) -> [bool; 2] {
        let mut blocked = [false; 2];
        for (axis, delta) in [dx, dy].into_iter().enumerate() {
            assert!(
                delta.abs() <= 4096,
                "2D motion exceeds 4096 units/tick; split or rescale your world"
            );
            for _ in 0..delta.abs() {
                let mut next = *self;
                if axis == 0 {
                    next.x += delta.signum();
                } else {
                    next.y += delta.signum();
                }
                if walls.iter().any(|w| next.overlaps(*w)) {
                    blocked[axis] = true;
                    break;
                }
                *self = next;
            }
        }
        blocked
    }
}
/// Simple deterministic kinematic physics; velocities/gravity are integer units per tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Body {
    pub bounds: Rect,
    pub velocity: Point,
    pub grounded: bool,
}
impl Body {
    pub fn step(&mut self, gravity: i32, walls: &[Rect]) {
        self.velocity.y = (self.velocity.y + gravity).clamp(-100, 100);
        let hits = self.bounds.slide(self.velocity.x, self.velocity.y, walls);
        self.grounded = hits[1] && self.velocity.y > 0;
        if hits[0] {
            self.velocity.x = 0;
        }
        if hits[1] {
            self.velocity.y = 0;
        }
    }
}
/// Edge-triggered contact, usable for entrances/pickups without rendering.
#[derive(Default)]
pub struct Trigger {
    inside: bool,
}
impl Trigger {
    pub fn enter(&mut self, area: Rect, actor: Rect) -> bool {
        let now = area.overlaps(actor);
        let entered = now && !self.inside;
        self.inside = now;
        entered
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Intent {
    pub x: i32,
    pub y: i32,
    pub pointer: Option<Point>,
    pub action: bool,
    /// Game-defined press edges; name each bit with a game-owned `u32` constant.
    /// The native client combines pending bits across device frames and delivers
    /// them once on the next fixed tick, independently of `action` and `pointer`.
    /// Repeated presses of the same bit before a tick coalesce; this is not an
    /// ordered text-edit or event queue. Older serialized intentions default to zero.
    #[serde(default, skip_serializing_if = "no_commands")]
    pub commands: u32,
    /// Radians accumulated across device frames and consumed once, for optional 3D look.
    #[serde(default)]
    pub look: [f32; 2],
    #[serde(default)]
    pub sprint: bool,
}

fn no_commands(commands: &u32) -> bool {
    *commands == 0
}

/// Collects portable device frames into fixed-tick intentions without losing edges.
///
/// Axes, pointer and sprint retain the latest frame; action, all 32 command bits
/// and look are consumed once. Clear on pause, focus loss, restart or load so
/// stale commands cannot reach a later simulation. Game rules consume only the
/// returned [`Intent`], making the same commands available to replay and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntentAccumulator {
    input: crate::runtime::InputAccumulator<Intent>,
    commands: u32,
}
impl IntentAccumulator {
    pub fn new() -> Self {
        Self::default()
    }
    /// Feed a device frame, including press edges and look deltas in `input`.
    pub fn feed(&mut self, mut input: Intent) {
        self.commands |= std::mem::take(&mut input.commands);
        let action = std::mem::take(&mut input.action);
        let look = std::mem::take(&mut input.look);
        self.input.feed(input, u32::from(action), look);
    }
    /// Consume pending edges/look, keeping held axes, pointer and sprint.
    pub fn take_tick(&mut self) -> Intent {
        let tick = self.input.take_tick();
        Intent {
            action: tick.pressed(1),
            commands: std::mem::take(&mut self.commands),
            look: tick.look,
            ..tick.held
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
/// Checked authored loop bank, shared by native presentation clients.
pub struct AudioBankSpec {
    pub id: &'static str,
    pub root: &'static str,
    /// Music toggle controls score banks; Sound controls ambience banks and effects.
    pub music: bool,
}
/// A game only owns rules and read-only presentation. The client owns devices, storage and timing.
pub trait GameLogic: Snapshot<Input = Intent> + Sized {
    const ID: &'static str;
    const TITLE: &'static str;
    const CONTROLS: &'static str;
    const VERIFY_TICKS: u32;
    fn new(seed: u64) -> Self;
    fn tick(&self) -> u32;
    /// Default restart begins a fresh game; persistent utilities may keep their library.
    fn restart(&mut self) {
        *self = Self::new(7);
    }
    /// Only click/tap presses supply pointer action targets; default retains continuous aiming.
    fn pointer_target_only_on_press() -> bool {
        false
    }
    fn outcome(&self) -> &'static str;
    /// A public-input playthrough, also consumed by native/headless and browser verification.
    fn verification_input(tick: u32) -> Intent;
    /// One meaningful real-device interaction, verified separately from automated replay.
    fn probe_input() -> Intent;
    fn probe_success(&self) -> bool;
    /// Read-only marker retained for game source compatibility after browser telemetry retirement.
    /// Ordinary games need no marker; native performance tooling is independent of simulation.
    fn streaming_marker(&self) -> (i64, i64) {
        (0, 0)
    }
    fn audio_banks() -> &'static [AudioBankSpec] {
        &[]
    }
    fn audio_level(&self, _bank: &str, _layer: &str) -> f32 {
        0.25
    }
    /// Presentation events; never feed audio/particles back into authoritative state.
    fn take_cues(&mut self) -> Vec<usize> {
        Vec::new()
    }
    /// Position of read-only cue feedback; defaults to the existing canvas center.
    fn cue_point(&self, _cue: usize) -> Point {
        Point::new(400, 200)
    }
}
pub fn verify<G: GameLogic>() -> (u64, &'static str) {
    let mut game = G::new(7);
    for tick in 0..G::VERIFY_TICKS {
        game.step(&G::verification_input(tick));
    }
    (game.state_hash(), game.outcome())
}
pub fn hash_json(value: &impl Serialize) -> u64 {
    StateHasher::new()
        .bytes(&serde_json::to_vec(value).expect("simulation state must serialize"))
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_edges_survive_fast_frames_and_are_consumed_once() {
        const HOLD: u32 = 1;
        const REVERSE: u32 = 1 << 31;
        let mut input = IntentAccumulator::new();
        input.feed(Intent {
            x: -1,
            action: true,
            commands: HOLD,
            look: [0.1, 0.],
            ..Default::default()
        });
        // No tick ran before a different device (e.g. controller) supplied an edge.
        input.feed(Intent {
            x: 1,
            commands: REVERSE | HOLD,
            look: [0., 0.2],
            pointer: Some(Point::new(20, 30)),
            ..Default::default()
        });
        let first = input.take_tick();
        assert!(first.action);
        assert_eq!(first.commands, HOLD | REVERSE);
        assert_eq!(first.look, [0.1, 0.2]);
        for _ in 0..3 {
            let next = input.take_tick();
            assert!(!next.action);
            assert_eq!(next.commands, 0);
            assert_eq!(next.look, [0., 0.]);
            assert_eq!(next.x, 1);
            assert_eq!(next.pointer, first.pointer);
        }
    }
    #[test]
    fn old_intentions_default_commands_and_new_commands_replay_losslessly() {
        let old: Intent =
            serde_json::from_str(r#"{"x":1,"y":0,"pointer":null,"action":true}"#).unwrap();
        assert_eq!(old.commands, 0);
        assert_eq!(
            serde_json::to_string(&old).unwrap(),
            r#"{"x":1,"y":0,"pointer":null,"action":true,"look":[0.0,0.0],"sprint":false}"#,
            "neutral command bits retain the old serialized input shape"
        );
        let mut input = IntentAccumulator::new();
        input.feed(Intent {
            commands: u32::MAX,
            ..old
        });
        let recorded = serde_json::to_vec(&input.take_tick()).unwrap();
        let replay: Intent = serde_json::from_slice(&recorded).unwrap();
        assert_eq!(replay.commands, u32::MAX);
        assert!(replay.action);
        assert_eq!(replay.x, 1);
        input.feed(replay);
        input.clear();
        assert_eq!(input.take_tick().commands, 0);
    }
    #[test]
    fn command_driven_rules_replay_and_resume_through_engine_snapshots() {
        const HOLD: u32 = 1;
        const REVERSE: u32 = 1 << 31;
        #[derive(Default)]
        struct Puzzle([i32; 4]);
        impl Simulation for Puzzle {
            type Input = Intent;
            fn step(&mut self, input: &Intent) {
                self.0[0] += input.x;
                self.0[1] += i32::from(input.action);
                self.0[2] += i32::from(input.commands & HOLD != 0);
                self.0[3] -= i32::from(input.commands & REVERSE != 0);
            }
            fn state_hash(&self) -> u64 {
                hash_json(&self.0)
            }
        }
        impl Snapshot for Puzzle {
            const KIND: &'static str = "portable-command-test";
            type State = [i32; 4];
            fn capture(&self) -> Self::State {
                self.0
            }
            fn restore(&mut self, state: Self::State) -> Result<(), String> {
                self.0 = state;
                Ok(())
            }
        }
        let mut input = IntentAccumulator::new();
        input.feed(Intent {
            commands: HOLD,
            ..Default::default()
        });
        input.feed(Intent {
            x: 1,
            action: true,
            commands: REVERSE,
            ..Default::default()
        });
        let route = [input.take_tick(), input.take_tick(), input.take_tick()];
        let recorded = serde_json::to_vec(&route).unwrap();
        let replay: Vec<Intent> = serde_json::from_slice(&recorded).unwrap();
        crate::runtime::assert_deterministic(Puzzle::default, &replay);
        crate::runtime::snapshot::assert_resumes_exactly(Puzzle::default, &replay, 1);
        let mut puzzle = Puzzle::default();
        for tick in replay {
            puzzle.step(&tick);
        }
        assert_eq!(puzzle.0, [3, 1, 1, -1]);
    }
    #[test]
    fn swept_collision_cannot_tunnel_and_slides_along_walls() {
        let mut a = Rect::new(0, 0, 10, 10);
        let wall = Rect::new(20, 0, 1, 100);
        assert_eq!(a.slide(100, 8, &[wall]), [true, false]);
        assert_eq!(a, Rect::new(10, 8, 10, 10));
        assert!(!a.overlaps(wall));
        assert!(!a.contains(Point::new(20, 8)));
    }
    #[test]
    fn physics_and_trigger_are_headless() {
        let mut b = Body {
            bounds: Rect::new(0, 0, 10, 10),
            velocity: Point::default(),
            grounded: false,
        };
        for _ in 0..60 {
            b.step(1, &[Rect::new(-100, 40, 200, 10)]);
        }
        assert_eq!(b.bounds.y, 30);
        assert!(b.grounded);
        let mut trigger = Trigger::default();
        assert!(trigger.enter(b.bounds, b.bounds));
        assert!(!trigger.enter(b.bounds, b.bounds));
        assert!(!trigger.enter(b.bounds, Rect::default()));
        assert!(trigger.enter(b.bounds, b.bounds));
    }
}
