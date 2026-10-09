//! Small 2D facade: integer rules, logical-pixel presentation and shared runtime services.
pub use crate::runtime::{Simulation, Snapshot, StateHasher};
use serde::{Deserialize, Serialize};
#[cfg(feature = "two-d")]
pub mod client;
#[cfg(feature = "two-d")]
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
    /// Radians accumulated across device frames and consumed once, for optional 3D look.
    #[serde(default)]
    pub look: [f32; 2],
    #[serde(default)]
    pub sprint: bool,
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
